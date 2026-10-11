//! Thread-safe shared access to one [`Engine`].

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::Value;
use viamind_doc::Project;

use crate::command::{Command, CommandSpec};
use crate::engine::{Engine, Tx};
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

    /// Specs of every accepted command, ordered by name (MCP tool definitions).
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.lock().specs()
    }

    /// Runs the command called `name` with JSON `input` as one transaction.
    pub fn execute(&self, name: &str, input: Value, origin: Origin) -> Result<ExecResult, EngineError> {
        self.lock().execute(name, input, origin)
    }

    /// Runs a typed command as one transaction.
    pub fn execute_typed<C: Command>(&self, command: C, origin: Origin) -> Result<ExecResult, EngineError> {
        self.lock().execute_typed(command, origin)
    }

    /// Runs `calls` as one all-or-nothing transaction (see [`Engine::execute_batch`]).
    pub fn execute_batch(&self, calls: Vec<CommandCall>, origin: Origin, label: Option<&str>) -> Result<ExecResult, EngineError> {
        self.lock().execute_batch(calls, origin, label)
    }

    /// Runs `f` as one transaction (see [`Engine::transaction`]). The engine is
    /// locked while `f` runs; do not call this handle from inside `f`.
    pub fn transaction<R>(
        &self,
        origin: Origin,
        label: &str,
        f: impl FnOnce(&mut Tx<'_>) -> Result<R, EngineError>,
    ) -> Result<(R, ExecResult), EngineError> {
        self.lock().transaction(origin, label, f)
    }

    /// Undoes the most recent transaction, whoever made it.
    pub fn undo(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        self.lock().undo(origin)
    }

    /// Redoes the most recently undone transaction.
    pub fn redo(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        self.lock().redo(origin)
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

    /// Runs `f` with the locked engine, e.g. to inspect the undo history.
    pub fn with_engine<R>(&self, f: impl FnOnce(&Engine) -> R) -> R {
        f(&self.lock())
    }
}
