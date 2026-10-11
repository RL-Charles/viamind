//! Thread-safe shared access to one [`Engine`].

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::Value;
use viamind_doc::Project;

use crate::command::{Command, CommandSpec};
use crate::engine::{Engine, Tx};
use crate::event::ChangeEvent;
use crate::registry::{CommandRegistry, EngineError};
use crate::transaction::{CommandCall, ExecResult, Origin, TxId, TxInfo};

/// Cheap-to-clone, thread-safe handle to one engine.
///
/// The egui UI thread and the tokio MCP server each hold a clone and call the
/// same methods; calls are serialised by an internal mutex, so every transaction
/// sees and leaves a consistent project. Keep closures passed to
/// [`read`](EngineHandle::read) and [`transaction`](EngineHandle::transaction) short.
#[derive(Clone, Debug)]
pub struct EngineHandle {
    engine: Arc<Mutex<Engine>>,
}

impl EngineHandle {
    /// A handle to a new engine for `project` with the built-in commands.
    pub fn new(project: Project) -> EngineHandle {
        EngineHandle::from_engine(Engine::new(project))
    }

    /// A handle to a new engine for `project` accepting the commands in `registry`.
    pub fn with_registry(project: Project, registry: CommandRegistry) -> EngineHandle {
        EngineHandle::from_engine(Engine::with_registry(project, registry))
    }

    /// Wraps an existing engine.
    pub fn from_engine(engine: Engine) -> EngineHandle {
        EngineHandle { engine: Arc::new(Mutex::new(engine)) }
    }

    /// Locks the engine. A panic inside a command poisons the mutex; the engine
    /// is still usable afterwards, so the poison is ignored.
    fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs a mutating operation under the lock, then runs any due `on_change`
    /// callbacks after the lock is released (so they may call back into the handle).
    fn mutate<R>(&self, f: impl FnOnce(&mut Engine) -> R) -> R {
        let (result, wakers) = {
            let mut engine = self.lock();
            let result = f(&mut engine);
            (result, engine.take_due_wakers())
        };
        for waker in wakers {
            waker();
        }
        result
    }

    /// Specs of every accepted command, ordered by name (MCP tool definitions).
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.lock().specs()
    }

    /// Current shared selection.
    pub fn selection(&self) -> crate::SelectionSnapshot {
        self.lock().selection().clone()
    }

    /// Updates transient selection and wakes listeners outside the engine lock.
    pub fn set_selection(&self, input: crate::SetSelection, origin: Origin) -> Result<crate::SelectionSnapshot, EngineError> {
        self.mutate(|engine| engine.set_selection(input, origin))
    }

    /// Runs the command called `name` with JSON `input` as one transaction.
    pub fn execute(&self, name: &str, input: Value, origin: Origin) -> Result<ExecResult, EngineError> {
        self.mutate(|engine| engine.execute(name, input, origin))
    }

    /// Runs a typed command as one transaction.
    pub fn execute_typed<C: Command>(&self, command: C, origin: Origin) -> Result<ExecResult, EngineError> {
        self.mutate(|engine| engine.execute_typed(command, origin))
    }

    /// Runs `calls` as one all-or-nothing transaction (see [`Engine::execute_batch`]).
    pub fn execute_batch(&self, calls: Vec<CommandCall>, origin: Origin, label: Option<&str>) -> Result<ExecResult, EngineError> {
        self.mutate(|engine| engine.execute_batch(calls, origin, label))
    }

    /// Runs `f` as one transaction (see [`Engine::transaction`]). The engine is
    /// locked while `f` runs; do not call this handle from inside `f`.
    pub fn transaction<R>(
        &self,
        origin: Origin,
        label: &str,
        f: impl FnOnce(&mut Tx<'_>) -> Result<R, EngineError>,
    ) -> Result<(R, ExecResult), EngineError> {
        self.mutate(|engine| engine.transaction(origin, label, f))
    }

    /// Undoes the most recent transaction, whoever made it.
    pub fn undo(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        self.mutate(|engine| engine.undo(origin))
    }

    /// Redoes the most recently undone transaction.
    pub fn redo(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        self.mutate(|engine| engine.redo(origin))
    }

    /// Undoes `origin`'s most recent transaction, leaving later changes by others
    /// in place (see [`Engine::undo_own`]). This is the agent-facing undo.
    pub fn undo_own(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        self.mutate(|engine| engine.undo_own(origin))
    }

    /// Redoes the transaction `origin` most recently undid (see [`Engine::redo_own`]).
    pub fn redo_own(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        self.mutate(|engine| engine.redo_own(origin))
    }

    /// The transaction [`undo_own`](EngineHandle::undo_own) would undo next for `origin`.
    pub fn next_undo_own(&self, origin: &Origin) -> Option<TxInfo> {
        self.lock().next_undo_own(origin)
    }

    /// The transaction [`redo_own`](EngineHandle::redo_own) would redo next for `origin`.
    pub fn next_redo_own(&self, origin: &Origin) -> Option<TxInfo> {
        self.lock().next_redo_own(origin)
    }

    /// Whether there is a transaction to undo.
    pub fn can_undo(&self) -> bool {
        self.lock().can_undo()
    }

    /// Whether there is a transaction to redo.
    pub fn can_redo(&self) -> bool {
        self.lock().can_redo()
    }

    /// The transaction undo would undo next (for an "Undo …" menu label).
    pub fn next_undo(&self) -> Option<TxInfo> {
        self.lock().next_undo()
    }

    /// The transaction redo would redo next.
    pub fn next_redo(&self) -> Option<TxInfo> {
        self.lock().next_redo()
    }

    /// Reads the current project. The engine is locked while `f` runs; do not
    /// call this handle from inside `f`.
    pub fn read<R>(&self, f: impl FnOnce(&Project) -> R) -> R {
        f(self.lock().project())
    }

    /// Queries board parts under a single read lock, without emitting events or changing history.
    pub fn query_parts(&self, query: &crate::PartQuery) -> Result<crate::PartQueryResult, crate::CommandError> {
        self.read(|project| query.run(project))
    }

    /// Runs `f` with the locked engine, e.g. to inspect the undo history.
    pub fn with_engine<R>(&self, f: impl FnOnce(&Engine) -> R) -> R {
        f(&self.lock())
    }

    /// Subscribes to change events: one [`ChangeEvent`] per committed, undone or
    /// redone transaction, in commit order, from every origin. The UI and the MCP
    /// server both use this stream.
    ///
    /// Events queue until received; drop the receiver to unsubscribe. From async
    /// code, pair it with [`on_change`](EngineHandle::on_change) (e.g. to wake a
    /// `tokio::sync::Notify`) and drain it with `try_recv`.
    pub fn subscribe(&self) -> Receiver<ChangeEvent> {
        self.lock().subscribe()
    }

    /// Registers `f` to run after every change, outside the engine lock. Meant
    /// as a cheap waker, e.g. egui's `ctx.request_repaint()`; read the details
    /// from a [`subscribe`](EngineHandle::subscribe) receiver.
    pub fn on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        self.lock().on_change(f);
    }
}
