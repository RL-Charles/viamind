//! The engine: one project, its command registry and its undo/redo history.

use serde_json::Value;
use viamind_doc::Project;

use crate::command::{Command, CommandSpec};
use crate::delta::{Delta, apply_all, coalesce, invert};
use crate::registry::{CommandRegistry, EngineError, apply_typed};
use crate::transaction::{CommandCall, ExecResult, Origin, Transaction, TxId, TxInfo};

/// Owns a project and is the only thing that changes it.
///
/// Every edit runs as a transaction: one or more commands that commit together
/// and undo as one step. Undo and redo are per transaction and independent of
/// origin, so the user can undo an agent's edit. Single-threaded; share it
/// between the UI and the MCP server through [`crate::EngineHandle`].
#[derive(Debug)]
pub struct Engine {
    project: Project,
    registry: CommandRegistry,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    next_tx: u64,
}

impl Engine {
    /// An engine for `project` with the built-in commands.
    pub fn new(project: Project) -> Engine {
        Engine::with_registry(project, CommandRegistry::builtin())
    }

    /// An engine for `project` accepting the commands in `registry`.
    pub fn with_registry(project: Project, registry: CommandRegistry) -> Engine {
        Engine { project, registry, undo: Vec::new(), redo: Vec::new(), next_tx: 1 }
    }

    /// The current project.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The command registry.
    pub fn registry(&self) -> &CommandRegistry {
        &self.registry
    }

    /// Specs of every accepted command, ordered by name.
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.registry.specs()
    }

    /// Runs the command called `name` with JSON `input` as one transaction.
    pub fn execute(&mut self, name: &str, input: Value, origin: Origin) -> Result<ExecResult, EngineError> {
        self.transaction(origin, name, |tx| tx.execute(name, input).map(drop)).map(|((), result)| result)
    }

    /// Runs a typed command as one transaction.
    pub fn execute_typed<C: Command>(&mut self, command: C, origin: Origin) -> Result<ExecResult, EngineError> {
        self.transaction(origin, C::NAME, |tx| tx.execute_typed(command).map(drop)).map(|((), result)| result)
    }

    /// Runs `calls` in order as one transaction: all of them commit and undo as one
    /// step, or, if any fails, none of them changes the project.
    ///
    /// The label is the command names joined with `+` unless `label` is given.
    pub fn execute_batch(&mut self, calls: Vec<CommandCall>, origin: Origin, label: Option<&str>) -> Result<ExecResult, EngineError> {
        let label = label.map_or_else(|| calls.iter().map(|call| call.name.as_str()).collect::<Vec<_>>().join("+"), str::to_owned);
        let run = |tx: &mut Tx<'_>| {
            for (index, call) in calls.into_iter().enumerate() {
                tx.execute(&call.name, call.input).map_err(|error| EngineError::BatchStep { index, error: Box::new(error) })?;
            }
            Ok(())
        };
        self.transaction(origin, &label, run).map(|((), result)| result)
    }

    /// Runs `f` as one transaction: every command it executes through the [`Tx`]
    /// commits together and undoes as one step. If `f` returns an error, every
    /// change it made is rolled back and the error is returned.
    ///
    /// A transaction that changes nothing gets an id but is not added to the
    /// history (and does not clear the redo stack).
    pub fn transaction<R>(
        &mut self,
        origin: Origin,
        label: &str,
        f: impl FnOnce(&mut Tx<'_>) -> Result<R, EngineError>,
    ) -> Result<(R, ExecResult), EngineError> {
        let mut tx = Tx { project: &mut self.project, registry: &self.registry, deltas: Vec::new() };
        let value = match f(&mut tx) {
            Ok(value) => value,
            Err(error) => {
                let undo = invert(&tx.deltas);
                apply_all(tx.project, &undo);
                return Err(error);
            }
        };
        let deltas = coalesce(tx.deltas);
        let id = TxId(self.next_tx);
        self.next_tx += 1;
        let result = ExecResult::new(id, deltas.clone());
        if !deltas.is_empty() {
            self.redo.clear();
            self.undo.push(Transaction { id, origin, label: label.to_owned(), deltas });
        }
        Ok((value, result))
    }

    /// Undoes the most recent transaction, whoever made it. Returns its id, or
    /// `None` if there is nothing to undo. `origin` is who asked for the undo.
    pub fn undo(&mut self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        let _ = origin;
        let Some(tx) = self.undo.pop() else { return Ok(None) };
        apply_all(&mut self.project, &invert(&tx.deltas));
        let id = tx.id;
        self.redo.push(tx);
        Ok(Some(id))
    }

    /// Redoes the most recently undone transaction. Returns its id, or `None` if
    /// there is nothing to redo. `origin` is who asked for the redo.
    pub fn redo(&mut self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        let _ = origin;
        let Some(tx) = self.redo.pop() else { return Ok(None) };
        apply_all(&mut self.project, &tx.deltas);
        let id = tx.id;
        self.undo.push(tx);
        Ok(Some(id))
    }

    /// Whether there is a transaction to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is a transaction to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The transaction [`undo`](Engine::undo) would undo next.
    pub fn next_undo(&self) -> Option<TxInfo> {
        self.undo.last().map(TxInfo::from)
    }

    /// The transaction [`redo`](Engine::redo) would redo next.
    pub fn next_redo(&self) -> Option<TxInfo> {
        self.redo.last().map(TxInfo::from)
    }

    /// The undo history, oldest first.
    pub fn undo_history(&self) -> &[Transaction] {
        &self.undo
    }

    /// The redo history; the last entry is redone first.
    pub fn redo_history(&self) -> &[Transaction] {
        &self.redo
    }
}

/// An open transaction, handed to the closure of [`Engine::transaction`].
pub struct Tx<'a> {
    project: &'a mut Project,
    registry: &'a CommandRegistry,
    deltas: Vec<Delta>,
}

impl Tx<'_> {
    /// The project as changed so far by this transaction.
    pub fn project(&self) -> &Project {
        self.project
    }

    /// Runs the command called `name` with JSON `input` inside this transaction
    /// and returns the deltas of this step.
    pub fn execute(&mut self, name: &str, input: Value) -> Result<Vec<Delta>, EngineError> {
        let deltas = self.registry.dispatch(name, input, self.project)?;
        self.deltas.extend_from_slice(&deltas);
        Ok(deltas)
    }

    /// Runs a typed command inside this transaction and returns the deltas of this step.
    pub fn execute_typed<C: Command>(&mut self, command: C) -> Result<Vec<Delta>, EngineError> {
        let deltas = apply_typed(command, self.project)?;
        self.deltas.extend_from_slice(&deltas);
        Ok(deltas)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::commands::{AddPart, MovePart};

    fn add(designator: &str) -> AddPart {
        AddPart {
            designator: designator.into(),
            value: "1k".into(),
            footprint: "R_0603".into(),
            x_mm: 0.0,
            y_mm: 0.0,
            rotation_deg: None,
            side: None,
        }
    }

    #[test]
    fn grouped_commands_undo_as_one_step() {
        let mut engine = Engine::new(Project::new("t"));
        let empty = engine.project().clone();
        let ((), result) = engine
            .transaction(Origin::User, "place three", |tx| {
                let r1 = tx.execute_typed(add("R1"))?[0].id();
                tx.execute_typed(add("R2"))?;
                tx.execute("add_part", json!({ "designator": "R3", "value": "", "footprint": "R_0603", "x_mm": 1, "y_mm": 1 }))?;
                tx.execute_typed(MovePart { id: r1, x_mm: 5.0, y_mm: 5.0 })?;
                Ok(())
            })
            .unwrap();
        // Four commands, three objects: the move is merged into R1's creation.
        assert_eq!(result.deltas.len(), 3);
        assert_eq!(result.created.len(), 3);
        assert_eq!(engine.project().board.parts.len(), 3);
        let full = engine.project().clone();
        assert_eq!(engine.next_undo().unwrap().label, "place three");

        assert_eq!(engine.undo(Origin::User).unwrap(), Some(result.tx));
        assert_eq!(engine.project(), &empty);
        assert_eq!(engine.undo(Origin::User).unwrap(), None);
        assert_eq!(engine.redo(Origin::User).unwrap(), Some(result.tx));
        assert_eq!(engine.project(), &full);
        assert_eq!(engine.redo(Origin::User).unwrap(), None);
    }

    #[test]
    fn failed_transaction_rolls_back() {
        let mut engine = Engine::new(Project::new("t"));
        engine.execute_typed(add("R1"), Origin::User).unwrap();
        let before = engine.project().clone();
        let calls = vec![
            CommandCall::new("add_part", json!({ "designator": "R2", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 })),
            CommandCall::new("add_part", json!({ "designator": "R1", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 })),
        ];
        let err = engine.execute_batch(calls, Origin::agent("a"), None).unwrap_err();
        assert!(err.to_string().starts_with("step 1 of the batch failed (no changes were made): `add_part` failed: designator `R1`"), "{err}");
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.undo_history().len(), 1);
    }

    #[test]
    fn batch_commits_as_one_transaction() {
        let mut engine = Engine::new(Project::new("t"));
        let calls = vec![
            CommandCall::new("add_part", json!({ "designator": "R1", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 })),
            CommandCall::new("add_part", json!({ "designator": "R2", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 })),
        ];
        let result = engine.execute_batch(calls, Origin::agent("a"), None).unwrap();
        assert_eq!(result.created.len(), 2);
        let info = engine.next_undo().unwrap();
        assert_eq!((info.label.as_str(), info.origin), ("add_part+add_part", Origin::agent("a")));
        engine.undo(Origin::User).unwrap();
        assert!(engine.project().board.parts.is_empty());
    }

    #[test]
    fn new_edit_clears_redo_but_no_op_does_not() {
        let mut engine = Engine::new(Project::new("t"));
        let r1 = engine.execute_typed(add("R1"), Origin::User).unwrap().created[0];
        engine.execute_typed(MovePart { id: r1, x_mm: 1.0, y_mm: 0.0 }, Origin::User).unwrap();
        engine.undo(Origin::User).unwrap();
        assert!(engine.can_redo());

        let noop = engine.execute_typed(MovePart { id: r1, x_mm: 0.0, y_mm: 0.0 }, Origin::User).unwrap();
        assert!(noop.deltas.is_empty());
        assert!(engine.can_redo(), "a no-op is not recorded");

        engine.execute_typed(add("R2"), Origin::User).unwrap();
        assert!(!engine.can_redo());
        assert_eq!(engine.redo(Origin::User).unwrap(), None);
    }

    #[test]
    fn tx_ids_increase() {
        let mut engine = Engine::new(Project::new("t"));
        let a = engine.execute_typed(add("R1"), Origin::User).unwrap().tx;
        let b = engine.execute_typed(add("R2"), Origin::User).unwrap().tx;
        assert!(b > a);
        assert_eq!(a.to_string(), "tx1");
        assert_eq!(serde_json::to_string(&Origin::agent("claude")).unwrap(), r#"{"kind":"agent","id":"claude"}"#);
        assert_eq!(serde_json::to_string(&Origin::User).unwrap(), r#"{"kind":"user"}"#);
    }
}
