//! The engine: one project, its command registry and its undo/redo history.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use serde_json::Value;
use viamind_doc::{ObjectId, Project};

use crate::command::{Command, CommandSpec};
use crate::delta::{Delta, apply_all, coalesce, invert};
use crate::event::{ChangeEvent, ChangeKind, Notifier, Waker};
use crate::log::EventLog;
use crate::registry::{CommandRegistry, EngineError, apply_typed};
use crate::transaction::{CommandCall, ExecResult, Origin, Transaction, TxId, TxInfo};

/// Owns a project and is the only thing that changes it.
///
/// Every edit runs as a transaction: one or more commands that commit together
/// and undo as one step. Undo and redo are per transaction and independent of
/// origin, so the user can undo an agent's edit. Every change is sent to
/// subscribers as a [`ChangeEvent`] and appended to the [`EventLog`].
/// Single-threaded; share it between the UI and the MCP server through
/// [`crate::EngineHandle`].
#[derive(Debug)]
pub struct Engine {
    project: Project,
    registry: CommandRegistry,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    next_tx: u64,
    notifier: Notifier,
    log: EventLog,
    selection: crate::SelectionSnapshot,
    session: crate::session::SessionJournal,
}

impl Engine {
    /// An engine for `project` with the built-in commands.
    pub fn new(project: Project) -> Engine {
        Engine::with_registry(project, CommandRegistry::builtin())
    }

    /// An engine for `project` accepting the commands in `registry`.
    pub fn with_registry(project: Project, registry: CommandRegistry) -> Engine {
        Engine {
            registry,
            selection: crate::SelectionSnapshot::default(),
            session: crate::session::SessionJournal::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            next_tx: 1,
            notifier: Notifier::default(),
            log: EventLog::new(project.clone()),
            project,
        }
    }

    /// The current project.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Current transient selection, shared by all clients.
    pub fn selection(&self) -> &crate::SelectionSnapshot {
        &self.selection
    }

    /// Current project and selection paired with their session cursor.
    pub fn session_state(&self) -> crate::SessionSnapshot {
        crate::SessionSnapshot { project: self.project.clone(), selection: self.selection.clone(), cursor: self.event_cursor() }
    }

    /// The current session tail, independent of document transaction IDs.
    pub fn event_cursor(&self) -> crate::EventCursor {
        self.session.cursor()
    }

    /// Read one immediate session event page. Use the shared handle for long polling.
    pub fn events_after(&self, after: crate::EventCursor, limit: usize) -> Result<crate::EventBatch, crate::SessionError> {
        crate::WatchEvents { limit, ..Default::default() }.validate()?;
        self.session.read(after, limit)
    }

    /// Updates selection atomically, validates IDs, and wakes repaint listeners.
    /// Does not add a design transaction or clear redo history.
    pub fn set_selection(&mut self, input: crate::SetSelection, origin: Origin) -> Result<crate::SelectionSnapshot, EngineError> {
        if let Some(id) = input.ids.iter().find(|id| !self.project.board.parts.contains_key(id)) {
            return Err(EngineError::InvalidInput {
                command: "set_selection".into(),
                message: format!("unknown board part {id}; call get_design for current IDs"),
            });
        }
        let mut ids: BTreeSet<_> = self.selection.ids.iter().copied().collect();
        let supplied: BTreeSet<_> = input.ids.into_iter().collect();
        match input.mode {
            crate::SelectionMode::Replace => ids = supplied,
            crate::SelectionMode::Add => ids.extend(supplied),
            crate::SelectionMode::Remove => ids.retain(|id| !supplied.contains(id)),
            crate::SelectionMode::Toggle => {
                for id in supplied {
                    if !ids.remove(&id) {
                        ids.insert(id);
                    }
                }
            }
        }
        let ids: Vec<_> = ids.into_iter().collect();
        if self.selection.ids != ids {
            self.selection.ids = ids;
            self.selection.revision += 1;
            self.selection.origin = Some(origin);
            self.session.push(crate::SessionEvent::Selection(self.selection.clone()));
            self.notifier.wake();
        }
        Ok(self.selection.clone())
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
            self.emit(ChangeEvent { tx: id, origin: origin.clone(), kind: ChangeKind::Applied, deltas: deltas.clone() });
            self.undo.push(Transaction { id, origin, label: label.to_owned(), deltas });
        }
        Ok((value, result))
    }

    /// Undoes the most recent transaction, whoever made it. Returns its id, or
    /// `None` if there is nothing to undo. `origin` is who asked for the undo.
    pub fn undo(&mut self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        let Some(tx) = self.undo.pop() else { return Ok(None) };
        let deltas = invert(&tx.deltas);
        apply_all(&mut self.project, &deltas);
        let id = tx.id;
        self.redo.push(tx);
        self.emit(ChangeEvent { tx: id, origin, kind: ChangeKind::Undone, deltas });
        Ok(Some(id))
    }

    /// Redoes the most recently undone transaction. Returns its id, or `None` if
    /// there is nothing to redo. `origin` is who asked for the redo.
    pub fn redo(&mut self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        let Some(tx) = self.redo.pop() else { return Ok(None) };
        apply_all(&mut self.project, &tx.deltas);
        let event = ChangeEvent { tx: tx.id, origin, kind: ChangeKind::Redone, deltas: tx.deltas.clone() };
        let id = tx.id;
        self.undo.push(tx);
        self.emit(event);
        Ok(Some(id))
    }

    /// Undoes the most recent transaction made by `origin`, leaving later
    /// transactions by others in place (an agent's "undo my last change").
    /// Returns its id, or `None` if `origin` has nothing left to undo.
    ///
    /// Fails with [`EngineError::UndoConflict`], changing nothing, if a later
    /// transaction still in the history touched any of the same objects.
    pub fn undo_own(&mut self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        let Some(index) = self.undo.iter().rposition(|tx| tx.origin == origin) else { return Ok(None) };
        let ids = object_ids(&self.undo[index]);
        if let Some(later) = self.undo[index + 1..].iter().find(|later| touches(later, &ids)) {
            return Err(EngineError::UndoConflict { tx: self.undo[index].id, blocked_by: later.id, blocked_by_origin: later.origin.clone() });
        }
        let tx = self.undo.remove(index);
        let deltas = invert(&tx.deltas);
        apply_all(&mut self.project, &deltas);
        let id = tx.id;
        self.redo.push(tx);
        self.emit(ChangeEvent { tx: id, origin, kind: ChangeKind::Undone, deltas });
        Ok(Some(id))
    }

    /// Redoes the transaction `origin` most recently undid, if it is still
    /// redoable (any new transaction clears the redo history). Returns its id, or
    /// `None` if `origin` has nothing to redo.
    ///
    /// Fails with [`EngineError::UndoConflict`], changing nothing, if a
    /// transaction undone before it (and not yet redone) touched the same objects.
    pub fn redo_own(&mut self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        let Some(index) = self.redo.iter().rposition(|tx| tx.origin == origin) else { return Ok(None) };
        let ids = object_ids(&self.redo[index]);
        if let Some(earlier) = self.redo[index + 1..].iter().find(|earlier| touches(earlier, &ids)) {
            return Err(EngineError::UndoConflict { tx: self.redo[index].id, blocked_by: earlier.id, blocked_by_origin: earlier.origin.clone() });
        }
        let tx = self.redo.remove(index);
        apply_all(&mut self.project, &tx.deltas);
        let event = ChangeEvent { tx: tx.id, origin, kind: ChangeKind::Redone, deltas: tx.deltas.clone() };
        let id = tx.id;
        self.undo.push(tx);
        self.emit(event);
        Ok(Some(id))
    }

    /// The transaction [`undo_own`](Engine::undo_own) would undo next for `origin`.
    pub fn next_undo_own(&self, origin: &Origin) -> Option<TxInfo> {
        self.undo.iter().rfind(|tx| tx.origin == *origin).map(TxInfo::from)
    }

    /// The transaction [`redo_own`](Engine::redo_own) would redo next for `origin`.
    pub fn next_redo_own(&self, origin: &Origin) -> Option<TxInfo> {
        self.redo.iter().rfind(|tx| tx.origin == *origin).map(TxInfo::from)
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

    /// The append-only log of every change event since the engine was created;
    /// [`EventLog::replay`] rebuilds the current project from it.
    pub fn log(&self) -> &EventLog {
        &self.log
    }

    /// Logs `event` and sends it to subscribers.
    fn emit(&mut self, event: ChangeEvent) {
        let before = self.selection.ids.len();
        self.selection.ids.retain(|id| self.project.board.parts.contains_key(id));
        if self.selection.ids.len() != before {
            self.selection.revision += 1;
            self.selection.origin = Some(event.origin.clone());
        }
        self.log.append(event.clone());
        self.session.push(crate::SessionEvent::Design(event.clone()));
        if self.selection.ids.len() != before {
            self.session.push(crate::SessionEvent::Selection(self.selection.clone()));
        }
        self.notifier.send(event);
    }

    /// Subscribes to change events: the receiver gets one [`ChangeEvent`] per
    /// committed, undone or redone transaction, in commit order. Transactions that
    /// change nothing send no event. Events queue until received, so drain the
    /// receiver regularly or drop it to unsubscribe.
    pub fn subscribe(&mut self) -> std::sync::mpsc::Receiver<ChangeEvent> {
        self.notifier.subscribe()
    }

    /// Number of live subscribers (closed receivers are dropped on the next event).
    pub fn subscriber_count(&self) -> usize {
        self.notifier.subscriber_count()
    }

    /// Registers `f` to run after every change (see [`crate::EngineHandle::on_change`]).
    pub fn on_change(&mut self, f: impl Fn() + Send + Sync + 'static) {
        self.notifier.on_change(Arc::new(f));
    }

    /// The `on_change` callbacks due because something changed since the last
    /// call. [`crate::EngineHandle`] runs them after releasing its lock; a caller
    /// driving an `Engine` directly runs them itself.
    pub fn take_due_wakers(&mut self) -> Vec<Waker> {
        self.notifier.take_due_wakers()
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

/// Ids of the objects `tx` changes.
fn object_ids(tx: &Transaction) -> HashSet<ObjectId> {
    tx.deltas.iter().map(Delta::id).collect()
}

/// Whether `tx` changes any of `ids`.
fn touches(tx: &Transaction, ids: &HashSet<ObjectId>) -> bool {
    tx.deltas.iter().any(|delta| ids.contains(&delta.id()))
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
