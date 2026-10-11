//! Thread-safe shared access to one [`Engine`].

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

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
    session_changed: Arc<Condvar>,
    saves: Arc<Mutex<()>>,
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
        EngineHandle { engine: Arc::new(Mutex::new(engine)), session_changed: Arc::new(Condvar::new()), saves: Arc::new(Mutex::new(())) }
    }

    /// Loads an existing native project and records its active absolute path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, viamind_doc::file::FileError> {
        let path = path.as_ref();
        let project = viamind_doc::file::load(path)?;
        let absolute = std::path::absolute(path).map_err(|source| viamind_doc::file::FileError::Io { path: path.to_owned(), source })?;
        let engine = Self::new(project.clone());
        engine.mutate(|engine| engine.mark_saved(project, absolute));
        Ok(engine)
    }

    /// Current native path and dirty state, shared by UI and MCP.
    pub fn document_status(&self) -> crate::DocumentStatus {
        self.lock().document_status()
    }

    /// Atomically saves a captured snapshot without blocking document edits during I/O.
    pub fn save_project(&self, input: crate::SaveProject) -> Result<crate::SaveResult, EngineError> {
        self.save_project_with(
            input,
            |project, path, overwrite| {
                if overwrite { viamind_doc::file::save(project, path) } else { viamind_doc::file::save_new(project, path) }
            },
        )
    }

    fn save_project_with(
        &self,
        input: crate::SaveProject,
        write: impl FnOnce(&Project, &Path, bool) -> Result<(), viamind_doc::file::FileError>,
    ) -> Result<crate::SaveResult, EngineError> {
        let invalid = |message: String| EngineError::InvalidInput { command: "save_project".into(), message };
        let save = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        let overwrite = input.path.is_none() || input.overwrite;
        let (snapshot, path) = {
            let engine = self.lock();
            let path =
                input.path.or_else(|| engine.document_status().path).ok_or_else(|| invalid("no active file; supply path to save_project".into()))?;
            if path.as_os_str().is_empty() {
                return Err(invalid("path must not be empty".into()));
            }
            (engine.session_state(), path)
        };
        let file_error = |message| EngineError::File { message };
        let path = std::path::absolute(path).map_err(|error| file_error(error.to_string()))?;
        write(&snapshot.project, &path, overwrite).map_err(|error| file_error(error.to_string()))?;
        let (dirty, wakers) = {
            let mut engine = self.lock();
            engine.mark_saved(snapshot.project, path.clone());
            (engine.document_status().dirty, engine.take_due_wakers())
        };
        // Reentrant repaint callbacks may save again: release both locks first.
        drop(save);
        for waker in wakers {
            waker();
        }
        Ok(crate::SaveResult { path, dirty, saved_cursor: snapshot.cursor })
    }

    /// Locks the engine. A panic inside a command poisons the mutex; the engine
    /// is still usable afterwards, so the poison is ignored.
    fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs a mutating operation under the lock, then runs any due `on_change`
    /// callbacks after the lock is released (so they may call back into the handle).
    fn mutate<R>(&self, f: impl FnOnce(&mut Engine) -> R) -> R {
        let (result, wakers, changed) = {
            let mut engine = self.lock();
            let before = engine.event_cursor();
            let result = f(&mut engine);
            let changed = engine.event_cursor() != before;
            (result, engine.take_due_wakers(), changed)
        };
        if changed {
            self.session_changed.notify_all();
        }
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

    /// Atomically reads project, selection and the cursor for subsequent changes.
    pub fn session_state(&self) -> crate::SessionSnapshot {
        self.lock().session_state()
    }

    /// Read session changes, waiting at most 30 seconds when no records are available.
    /// The lock is released while waiting; use a blocking worker from async code.
    pub fn watch_events(&self, input: crate::WatchEvents) -> Result<crate::EventBatch, crate::SessionError> {
        input.validate()?;
        let deadline = Instant::now() + Duration::from_millis(input.timeout_ms);
        let mut engine = self.lock();
        let after = input.after.unwrap_or_else(|| engine.event_cursor());
        loop {
            let batch = engine.events_after(after, input.limit)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !batch.events.is_empty() || remaining.is_zero() {
                return Ok(batch);
            }
            let (guard, _) = self.session_changed.wait_timeout(engine, remaining).unwrap_or_else(PoisonError::into_inner);
            engine = guard;
        }
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

#[cfg(test)]
mod save_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn edits_during_file_io_remain_dirty_and_are_not_in_the_saved_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("concurrent.viamind.json");
        let engine = EngineHandle::new(Project::new("snapshot"));
        let before = engine.session_state();
        let result = engine
            .save_project_with(crate::SaveProject { path: Some(path.clone()), overwrite: false }, |project, path, _| {
                // A real command can run while the captured snapshot is being written.
                engine
                    .execute(
                        "add_part",
                        serde_json::json!({ "designator": "R1", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 }),
                        Origin::agent("concurrent"),
                    )
                    .unwrap();
                viamind_doc::file::save_new(project, path)
            })
            .unwrap();
        assert!(result.dirty);
        assert_eq!(result.saved_cursor, before.cursor);
        assert_eq!(viamind_doc::file::load(path).unwrap(), before.project);
        assert_eq!(engine.read(|project| project.board.parts.len()), 1);
        assert!(engine.document_status().dirty);
    }

    #[test]
    fn saved_callbacks_run_after_both_engine_and_save_locks_are_released() {
        let directory = tempfile::tempdir().unwrap();
        let engine = EngineHandle::new(Project::new("reentrant save"));
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        let callback = engine.clone();
        engine.on_change(move || {
            if !flag.swap(true, Ordering::SeqCst) {
                assert!(!callback.document_status().dirty);
                callback.save_project(Default::default()).unwrap();
            }
        });
        engine.save_project(crate::SaveProject { path: Some(directory.path().join("callback.viamind.json")), overwrite: false }).unwrap();
        assert!(called.load(Ordering::SeqCst));
    }
}
