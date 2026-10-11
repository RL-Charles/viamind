//! The [`Command`] trait: the only way to change a [`Project`].

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use viamind_doc::{ObjectId, Project};

use crate::delta::Delta;

/// A typed, serialisable edit of the design model.
///
/// The input type *is* the command: its fields are the arguments, it is built
/// from JSON (serde) by agents and from Rust by the UI, and its JSON Schema
/// (schemars) is advertised to agents as the MCP tool input schema.
///
/// [`apply`](Command::apply) performs the edit and returns one [`Delta`] per
/// changed object. Commands must change the project only through
/// [`Delta::record_part`] and friends so the deltas describe the change exactly;
/// the engine then undoes it generically with [`crate::invert`], so commands have
/// no hand-written inverse.
///
/// On error, `apply` must leave the project unchanged: validate first, then
/// record changes.
pub trait Command: DeserializeOwned + JsonSchema + Send + 'static {
    /// Unique snake_case name, used as the MCP tool name (e.g. `"add_part"`).
    const NAME: &'static str;
    /// One-paragraph description for agents and the UI command palette.
    const DESCRIPTION: &'static str;

    /// Optional optimistic guard against the session cursor captured at transaction start.
    /// Commands requiring a cursor reject dispatch without a live engine context. All calls
    /// in one transaction compare the same initial cursor, including deliberate batch steps.
    fn expected_cursor(&self) -> Option<crate::EventCursor> {
        None
    }

    /// Performs the edit on `project` and returns what changed.
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError>;
}

/// Why a command was rejected. Messages are written for the caller (often an
/// agent) to act on; the project is unchanged when this is returned.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandError {
    /// The command referred to an object that does not exist.
    #[error("no {object} with id {id} exists")]
    NotFound {
        /// Kind of object looked up, e.g. `"part"`.
        object: String,
        /// The id that was not found.
        id: ObjectId,
    },
    /// An input field has an unusable value.
    #[error("invalid `{field}`: {message}")]
    Invalid {
        /// Name of the offending input field.
        field: String,
        /// What is wrong and what is accepted.
        message: String,
    },
    /// The edit would conflict with the current design (e.g. a duplicate designator).
    #[error("{message}")]
    Conflict {
        /// What conflicts and how to resolve it.
        message: String,
    },
}

impl CommandError {
    /// Shorthand for [`CommandError::NotFound`].
    pub fn not_found(object: impl Into<String>, id: ObjectId) -> CommandError {
        CommandError::NotFound { object: object.into(), id }
    }

    /// Shorthand for [`CommandError::Invalid`].
    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> CommandError {
        CommandError::Invalid { field: field.into(), message: message.into() }
    }

    /// Shorthand for [`CommandError::Conflict`].
    pub fn conflict(message: impl Into<String>) -> CommandError {
        CommandError::Conflict { message: message.into() }
    }
}

/// What a command is and what input it takes; becomes an MCP tool definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandSpec {
    /// The command name ([`Command::NAME`]).
    pub name: String,
    /// The description ([`Command::DESCRIPTION`]).
    pub description: String,
    /// JSON Schema (draft 2020-12, an object schema with all subschemas inlined)
    /// of the command's input.
    pub input_schema: Value,
}

impl CommandSpec {
    /// The spec of command type `C`.
    pub fn of<C: Command>() -> CommandSpec {
        CommandSpec { name: C::NAME.to_owned(), description: C::DESCRIPTION.to_owned(), input_schema: input_schema::<C>() }
    }
}

/// JSON Schema of `T` as used for command inputs: draft 2020-12 with every
/// subschema inlined (no `$ref`/`$defs`), which tool-calling clients handle best.
pub fn input_schema<T: JsonSchema>() -> Value {
    SchemaSettings::draft2020_12().with(|settings| settings.inline_subschemas = true).into_generator().into_root_schema_for::<T>().to_value()
}

#[cfg(test)]
mod tests {
    use viamind_doc::Sheet;

    use super::*;

    /// Renames a sheet (test-only command).
    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct RenameSheet {
        /// Sheet to rename.
        id: ObjectId,
        /// New name.
        name: String,
    }

    impl Command for RenameSheet {
        const NAME: &'static str = "rename_sheet";
        const DESCRIPTION: &'static str = "Renames a schematic sheet.";

        fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
            let mut sheet = project.schematic.sheet(&self.id).cloned().ok_or_else(|| CommandError::not_found("sheet", self.id))?;
            if self.name.trim().is_empty() {
                return Err(CommandError::invalid("name", "must not be empty"));
            }
            sheet.name = self.name;
            Ok(vec![Delta::record_sheet(project, self.id, Some(sheet))])
        }
    }

    #[test]
    fn spec_has_object_schema_from_the_type() {
        let spec = CommandSpec::of::<RenameSheet>();
        assert_eq!(spec.name, "rename_sheet");
        assert_eq!(spec.description, "Renames a schematic sheet.");
        let schema = &spec.input_schema;
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["id"]["type"], "string");
        assert_eq!(schema["properties"]["id"]["format"], "uuid");
        assert_eq!(schema["properties"]["name"]["description"], "New name.");
        assert_eq!(schema["additionalProperties"], false);
        let required: Vec<_> = schema["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(required, ["id", "name"]);
        assert!(!schema.to_string().contains("$ref"), "subschemas are inlined");
    }

    #[test]
    fn apply_then_invert_restores_exactly() {
        let mut project = Project::new("t");
        let sheet = Sheet::new("Old");
        project.schematic.sheets.insert(sheet.id, sheet.clone());
        let before = project.clone();

        let deltas = RenameSheet { id: sheet.id, name: "New".into() }.apply(&mut project).unwrap();
        assert_eq!(project.schematic.sheet(&sheet.id).unwrap().name, "New");
        crate::apply_all(&mut project, &crate::invert(&deltas));
        assert_eq!(project, before);
    }

    #[test]
    fn errors_leave_project_unchanged_and_read_well() {
        let mut project = Project::new("t");
        let sheet = Sheet::new("Old");
        project.schematic.sheets.insert(sheet.id, sheet.clone());
        let before = project.clone();

        let missing = ObjectId::new();
        let err = RenameSheet { id: missing, name: "x".into() }.apply(&mut project).unwrap_err();
        assert_eq!(err.to_string(), format!("no sheet with id {missing} exists"));
        let err = RenameSheet { id: sheet.id, name: " ".into() }.apply(&mut project).unwrap_err();
        assert_eq!(err.to_string(), "invalid `name`: must not be empty");
        assert_eq!(project, before);
        assert_eq!(CommandError::conflict("taken").to_string(), "taken");
        assert_eq!(serde_json::to_value(CommandError::conflict("taken")).unwrap()["kind"], "conflict");
    }
}
