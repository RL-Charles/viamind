//! Registry of command types, dispatching JSON input by command name.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;
use viamind_doc::Project;

use crate::command::{Command, CommandError, CommandSpec};
use crate::delta::Delta;
use crate::transaction::{Origin, TxId};

/// Why the engine could not run a command.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// Native project persistence failed; design and prior file status are unchanged.
    #[error("native project file error: {message}")]
    File {
        /// File path and actionable I/O or parsing failure.
        message: String,
    },
    /// No command with this name is registered.
    #[error("unknown command `{name}`; available commands: {}", available.join(", "))]
    UnknownCommand {
        /// The name that was asked for.
        name: String,
        /// Every registered command name, sorted.
        available: Vec<String>,
    },
    /// The JSON input does not match the command's input schema.
    #[error("invalid input for `{command}`: {message}")]
    InvalidInput {
        /// The command name.
        command: String,
        /// The deserialisation error, naming the offending field.
        message: String,
    },
    /// The command ran and rejected the edit; the project is unchanged.
    #[error("`{command}` failed: {error}")]
    Command {
        /// The command name.
        command: String,
        /// What the command reported.
        error: CommandError,
    },
    /// A step of a batch failed, so the whole batch was rolled back.
    #[error("step {index} of the batch failed (no changes were made): {error}")]
    BatchStep {
        /// Zero-based index of the failing call.
        index: usize,
        /// Why it failed.
        error: Box<EngineError>,
    },
    /// A selective undo/redo (`undo_own`, `redo_own`) was refused because another
    /// transaction changed the same objects; nothing was changed.
    #[error("cannot undo or redo {tx}: {blocked_by} by {blocked_by_origin} changed the same objects; undo that first, or edit the objects directly")]
    UndoConflict {
        /// The transaction that could not be undone or redone.
        tx: TxId,
        /// The transaction in the way.
        blocked_by: TxId,
        /// Who made the transaction in the way.
        blocked_by_origin: Origin,
    },
}

type RunFn = fn(Value, &mut Project, Option<crate::EventCursor>) -> Result<Vec<Delta>, EngineError>;

struct Entry {
    spec: CommandSpec,
    run: RunFn,
}

/// The set of commands the engine accepts by name, ordered by name.
///
/// Built from [`Command`] types; [`specs`](CommandRegistry::specs) lists them for
/// MCP and [`dispatch`](CommandRegistry::dispatch) runs one from JSON input.
#[derive(Default)]
pub struct CommandRegistry {
    commands: BTreeMap<&'static str, Entry>,
}

impl fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.commands.keys()).finish()
    }
}

impl CommandRegistry {
    /// An empty registry.
    pub fn new() -> CommandRegistry {
        CommandRegistry::default()
    }

    /// A registry with every built-in command (see [`crate::commands`]).
    pub fn builtin() -> CommandRegistry {
        use crate::commands::{AddPart, DeletePart, MovePart, SetBoardOutline, UpdatePart};
        CommandRegistry::new()
            .with::<AddPart>()
            .with::<MovePart>()
            .with::<DeletePart>()
            .with::<UpdatePart>()
            .with::<crate::SetPartAssembly>()
            .with::<SetBoardOutline>()
            .with::<crate::ImportBoardOutlineDxf>()
            .with::<crate::SetPartPads>()
            .with::<crate::AddTrack>()
            .with::<crate::DeleteTrack>()
            .with::<crate::AddVia>()
            .with::<crate::DeleteVia>()
            .with::<crate::TransformParts>()
            .with::<crate::RoutePolyline>()
            .with::<crate::RoutePadPolyline>()
            .with::<crate::RoutePadPosture>()
            .with::<crate::RoutePadWalkaround>()
            .with::<crate::RoutePadViaWalkaround>()
            .with::<crate::CleanupCardinalTracks>()
            .with::<crate::RoutePadViaPolyline>()
            .with::<crate::ApplyFootprint>()
            .with::<crate::AnnotateSchematic>()
            .with::<crate::EditSchematicSheet>()
            .with::<crate::DrawSchematicWirePolyline>()
            .with::<crate::PlaceSymbolTemplate>()
            .with::<crate::DeleteSchematicSheet>()
            .with::<crate::LinkSchematicSymbol>()
            .with::<crate::SynchronizeSchematicPcb>()
            .with::<crate::CreateSchematicPcbComponent>()
    }

    /// Adds command type `C`.
    ///
    /// # Panics
    ///
    /// If a command with the same name is already registered, or the name is not
    /// 1-64 characters of `[a-z0-9_]` (a programming error caught at start-up).
    pub fn register<C: Command>(&mut self) -> &mut CommandRegistry {
        let name = C::NAME;
        assert!(
            (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "command name `{name}` must be 1-64 characters of [a-z0-9_]"
        );
        assert!(!self.commands.contains_key(name), "command `{name}` is registered twice");
        self.commands.insert(name, Entry { spec: CommandSpec::of::<C>(), run: run::<C> });
        self
    }

    /// Builder form of [`register`](CommandRegistry::register).
    pub fn with<C: Command>(mut self) -> CommandRegistry {
        self.register::<C>();
        self
    }

    /// Specs of all registered commands, ordered by name.
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.commands.values().map(|entry| entry.spec.clone()).collect()
    }

    /// Spec of the command called `name`.
    pub fn spec(&self, name: &str) -> Option<&CommandSpec> {
        self.commands.get(name).map(|entry| &entry.spec)
    }

    /// Whether a command called `name` is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    /// Registered command names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.commands.keys().copied()
    }

    /// Parses `input` as the command called `name` and applies it to `project`.
    ///
    /// `null` input is treated as `{}` (for commands without arguments). On error
    /// the project is unchanged.
    pub fn dispatch(&self, name: &str, input: Value, project: &mut Project) -> Result<Vec<Delta>, EngineError> {
        self.dispatch_with_cursor(name, input, project, None)
    }

    pub(crate) fn dispatch_with_cursor(
        &self,
        name: &str,
        input: Value,
        project: &mut Project,
        cursor: Option<crate::EventCursor>,
    ) -> Result<Vec<Delta>, EngineError> {
        let entry = self
            .commands
            .get(name)
            .ok_or_else(|| EngineError::UnknownCommand { name: name.to_owned(), available: self.names().map(str::to_owned).collect() })?;
        (entry.run)(input, project, cursor)
    }
}

/// Type-erased entry point for command `C`.
fn run<C: Command>(input: Value, project: &mut Project, cursor: Option<crate::EventCursor>) -> Result<Vec<Delta>, EngineError> {
    let input = if input.is_null() { Value::Object(serde_json::Map::new()) } else { input };
    let command: C =
        serde_json::from_value(input).map_err(|err| EngineError::InvalidInput { command: C::NAME.to_owned(), message: err.to_string() })?;
    apply_typed(command, project, cursor)
}

/// Applies an already-built command, mapping its error to [`EngineError::Command`].
pub(crate) fn apply_typed<C: Command>(command: C, project: &mut Project, cursor: Option<crate::EventCursor>) -> Result<Vec<Delta>, EngineError> {
    if let Some(expected) = command.expected_cursor()
        && cursor != Some(expected)
    {
        return Err(EngineError::Command {
            command: C::NAME.into(),
            error: CommandError::conflict("session changed; read get_session_state and rebuild this command"),
        });
    }
    command.apply(project).map_err(|error| EngineError::Command { command: C::NAME.to_owned(), error })
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;
    use viamind_doc::Sheet;

    use super::*;

    /// Adds a sheet (test-only command).
    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct AddSheet {
        /// Sheet title.
        name: String,
    }

    impl Command for AddSheet {
        const NAME: &'static str = "add_sheet";
        const DESCRIPTION: &'static str = "Adds a schematic sheet.";

        fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
            let sheet = Sheet::new(self.name);
            Ok(vec![Delta::record_sheet(project, sheet.id, Some(sheet))])
        }
    }

    /// Does nothing (test-only command without arguments).
    #[derive(Deserialize, JsonSchema)]
    struct Noop {}

    impl Command for Noop {
        const NAME: &'static str = "noop";
        const DESCRIPTION: &'static str = "Does nothing.";

        fn apply(self, _project: &mut Project) -> Result<Vec<Delta>, CommandError> {
            Ok(Vec::new())
        }
    }

    fn registry() -> CommandRegistry {
        CommandRegistry::new().with::<Noop>().with::<AddSheet>()
    }

    #[test]
    fn lists_specs_in_name_order() {
        let registry = registry();
        let names: Vec<_> = registry.specs().into_iter().map(|spec| spec.name).collect();
        assert_eq!(names, ["add_sheet", "noop"]);
        assert!(registry.contains("noop") && !registry.contains("nope"));
        assert_eq!(registry.spec("add_sheet").unwrap().input_schema["properties"]["name"]["type"], "string");
        assert_eq!(format!("{registry:?}"), r#"["add_sheet", "noop"]"#);
    }

    #[test]
    fn dispatches_json_by_name() {
        let registry = registry();
        let mut project = Project::new("t");
        let deltas = registry.dispatch("add_sheet", json!({ "name": "Power" }), &mut project).unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(project.schematic.sheet(&deltas[0].id()).unwrap().name, "Power");
        assert_eq!(registry.dispatch("noop", Value::Null, &mut project).unwrap(), vec![]);
    }

    #[test]
    fn reports_unknown_commands_and_bad_input() {
        let registry = registry();
        let mut project = Project::new("t");
        let before = project.clone();

        let err = registry.dispatch("add_shet", json!({}), &mut project).unwrap_err();
        assert_eq!(err.to_string(), "unknown command `add_shet`; available commands: add_sheet, noop");

        let err = registry.dispatch("add_sheet", json!({ "title": "x" }), &mut project).unwrap_err();
        let EngineError::InvalidInput { command, message } = &err else { panic!("{err:?}") };
        assert_eq!(command, "add_sheet");
        assert!(message.contains("title"), "{message}");

        let err = registry.dispatch("add_sheet", json!({ "name": 3 }), &mut project).unwrap_err();
        assert!(err.to_string().starts_with("invalid input for `add_sheet`: invalid type"), "{err}");
        assert_eq!(project, before);
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn duplicate_names_panic() {
        let _ = CommandRegistry::new().with::<Noop>().with::<Noop>();
    }
}
