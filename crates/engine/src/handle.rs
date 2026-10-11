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
    pub(crate) engine: Arc<Mutex<Engine>>,
    session_changed: Arc<Condvar>,
    saves: Arc<Mutex<()>>,
    pub(crate) automatic_drc: Arc<crate::auto_drc::AutoDrcController>,
    pub(crate) agent_presence: Arc<Mutex<crate::agent_presence::PresenceStore>>,
    pub(crate) pcb_branches: Arc<Mutex<crate::pcb_branches::BranchStore>>,
    agent_policy: Arc<crate::agent_policy::AgentPolicy>,
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
        let engine = Arc::new(Mutex::new(engine));
        let automatic_drc = Arc::new(crate::auto_drc::AutoDrcController::new(&engine));
        EngineHandle {
            engine,
            automatic_drc,
            session_changed: Arc::new(Condvar::new()),
            saves: Arc::new(Mutex::new(())),
            agent_presence: Arc::default(),
            pcb_branches: Arc::default(),
            agent_policy: Arc::default(),
        }
    }

    pub fn command_hook_status(&self) -> crate::CommandHookStatus {
        self.lock().command_hook_status()
    }

    pub fn configure_command_hooks(&self, config: crate::CommandHookConfig) -> Result<crate::CommandHookStatus, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.configure_command_hooks(config))
    }

    pub fn register_pre_command_hook(&self, id: String, hook: crate::PreCommandHook) -> Result<(), EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.register_pre_command_hook(id, hook))
    }

    pub fn register_post_command_hook(&self, id: String, hook: crate::PostCommandHook) -> Result<(), EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.register_post_command_hook(id, hook))
    }

    pub fn remove_command_hook(&self, id: &str) -> Result<bool, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.remove_command_hook(id))
    }

    /// Trusted transient agent admission shared with desktop and MCP endpoints.
    pub fn agent_policy(&self) -> Arc<crate::agent_policy::AgentPolicy> {
        crate::command_hooks::read_allowed();
        Arc::clone(&self.agent_policy)
    }

    /// Configure transient shared automatic DRC without modifying design/history.
    pub fn configure_auto_drc(&self, config: crate::AutoDrcConfig) -> Result<crate::AutoDrcStatus, EngineError> {
        crate::command_hooks::check_write()?;
        let _engine = self.lock();
        self.automatic_drc.configure(config)
    }

    /// Read the current bounded automatic DRC status. No project copy is needed.
    pub fn auto_drc_status(&self) -> crate::AutoDrcStatus {
        crate::command_hooks::read_allowed();
        self.automatic_drc.status()
    }

    /// Atomic current automatic DRC status and its independent observation cursor.
    pub fn auto_drc_event_state(&self) -> crate::AutoDrcEventState {
        crate::command_hooks::read_allowed();
        self.automatic_drc.event_state()
    }

    /// Read bounded automatic DRC summaries, releasing the state mutex while waiting.
    /// This never takes the document or native file lock; use a blocking worker from async code.
    pub fn watch_auto_drc_events(&self, input: crate::WatchAutoDrcEvents) -> Result<crate::AutoDrcEventBatch, crate::AutoDrcEventError> {
        crate::command_hooks::check_write().map_err(|_| crate::AutoDrcEventError::InvalidInput("waiting inside command hooks is unavailable"))?;
        self.automatic_drc.watch_events(input)
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

    /// Publish an already staged export while native open/new/save operations are excluded.
    ///
    /// The closure receives the current active native path. Prepare and sync the export
    /// before calling this method; keep the closure limited to path checks and atomic
    /// publication. The document mutex is released before the closure, so ordinary
    /// commands remain available. The closure must not reenter native file operations.
    pub fn with_native_file_guard<R>(&self, publish: impl FnOnce(Option<&Path>) -> R) -> R {
        crate::command_hooks::write_allowed();
        let _hook_scope = crate::command_hooks::NativeHookScope::enter();
        let _files = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        let native = self.lock().document_status().path;
        publish(native.as_deref())
    }

    /// Atomically saves a captured snapshot without blocking document edits during I/O.
    pub fn save_project(&self, input: crate::SaveProject) -> Result<crate::SaveResult, EngineError> {
        crate::command_hooks::check_write()?;
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
        crate::command_hooks::check_write()?;
        let invalid = |message: String| EngineError::InvalidInput { command: "save_project".into(), message };
        let _hook_scope = crate::command_hooks::NativeHookScope::enter();
        let save = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        let overwrite = input.path.is_none() || input.overwrite;
        let (snapshot, path) = {
            let engine = self.lock();
            let path =
                input.path.or_else(|| engine.document_status().path).ok_or_else(|| invalid("no active file; supply path to save_project".into()))?;
            if path.as_os_str().is_empty() {
                return Err(invalid("path must not be empty".into()));
            }
            viamind_doc::file::validate_net_rules(engine.project()).map_err(|e| invalid(e.to_string()))?;
            viamind_doc::file::validate_assembly(engine.project()).map_err(|e| invalid(e.to_string()))?;
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

    /// Creates an unnamed document without replacing the shared handle or its listeners.
    pub fn new_project(&self, input: crate::NewProject, origin: Origin) -> Result<crate::SessionSnapshot, EngineError> {
        crate::command_hooks::check_write()?;
        if input.name.trim().is_empty() {
            return Err(EngineError::InvalidInput { command: "new_project".into(), message: "name must not be blank".into() });
        }
        self.replace_project(
            "new_project",
            crate::ReplaceGuard { discard_dirty: input.discard_dirty, expected_cursor: input.expected_cursor },
            origin,
            || Ok((Project::new(input.name), None)),
        )
    }

    /// Validates a native file before replacing the document; concurrent edits reject the replacement.
    pub fn open_project(&self, input: crate::OpenProject, origin: Origin) -> Result<crate::SessionSnapshot, EngineError> {
        crate::command_hooks::check_write()?;
        if input.path.as_os_str().is_empty() {
            return Err(EngineError::InvalidInput { command: "open_project".into(), message: "path must not be empty".into() });
        }
        self.replace_project(
            "open_project",
            crate::ReplaceGuard { discard_dirty: input.discard_dirty, expected_cursor: input.expected_cursor },
            origin,
            || {
                let path = std::path::absolute(input.path).map_err(|e| EngineError::File { message: e.to_string() })?;
                let project = viamind_doc::file::load(&path).map_err(|e| EngineError::File { message: e.to_string() })?;
                Ok((project, Some(path)))
            },
        )
    }

    /// Read-only lifecycle guard used before closing the desktop window.
    pub fn check_document_guard(&self, guard: &crate::ReplaceGuard) -> Result<(), EngineError> {
        crate::command_hooks::check_read()?;
        Self::check_replace(&self.lock(), guard, "close_project")
    }

    fn check_replace(engine: &Engine, guard: &crate::ReplaceGuard, command: &str) -> Result<(), EngineError> {
        let invalid = |message: &str| EngineError::InvalidInput { command: command.into(), message: message.into() };
        if guard.expected_cursor.is_some_and(|cursor| cursor != engine.event_cursor()) {
            return Err(invalid("document changed; call get_session_state and confirm again"));
        }
        if engine.document_status().dirty && (!guard.discard_dirty || guard.expected_cursor.is_none()) {
            return Err(invalid("unsaved changes; save first or supply discard_dirty and current expected_cursor from get_session_state"));
        }
        Ok(())
    }

    fn replace_project(
        &self,
        command: &str,
        guard: crate::ReplaceGuard,
        origin: Origin,
        load: impl FnOnce() -> Result<(Project, Option<std::path::PathBuf>), EngineError>,
    ) -> Result<crate::SessionSnapshot, EngineError> {
        self.replace_project_internal(command, guard, origin, None, false, load)
    }

    pub(crate) fn replace_imported_project(
        &self,
        expected_project: viamind_doc::ObjectId,
        expected_cursor: crate::EventCursor,
        origin: Origin,
        load: impl FnOnce() -> Result<Project, EngineError>,
    ) -> Result<crate::SessionSnapshot, EngineError> {
        self.replace_project_internal(
            "import_pcb",
            crate::ReplaceGuard { discard_dirty: false, expected_cursor: Some(expected_cursor) },
            origin,
            Some(expected_project),
            true,
            || Ok((load()?, None)),
        )
    }

    fn replace_project_internal(
        &self,
        command: &str,
        guard: crate::ReplaceGuard,
        origin: Origin,
        expected_project: Option<viamind_doc::ObjectId>,
        imported: bool,
        load: impl FnOnce() -> Result<(Project, Option<std::path::PathBuf>), EngineError>,
    ) -> Result<crate::SessionSnapshot, EngineError> {
        crate::command_hooks::check_write()?;
        let _hook_scope = crate::command_hooks::NativeHookScope::enter();
        let files = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        let before = {
            let engine = self.lock();
            Self::check_replace(&engine, &guard, command)?;
            if expected_project.is_some_and(|id| id != engine.project().id) {
                return Err(EngineError::InvalidInput { command: command.into(), message: "project changed; preview again".into() });
            }
            engine.event_cursor()
        };
        let (project, path) = load()?;
        let (snapshot, wakers) = {
            let mut engine = self.lock();
            Self::check_replace(&engine, &crate::ReplaceGuard { expected_cursor: Some(before), ..guard }, command)?;
            if expected_project.is_some_and(|id| id != engine.project().id) {
                return Err(EngineError::InvalidInput { command: command.into(), message: "project changed; preview again".into() });
            }
            engine.replace_document(project, path, origin);
            if imported {
                engine.mark_imported_unsaved();
            }
            self.automatic_drc.design_changed();
            (engine.session_state(), engine.take_due_wakers())
        };
        drop(files);
        self.session_changed.notify_all();
        for waker in wakers {
            waker();
        }
        Ok(snapshot)
    }

    /// Locks the engine. A panic inside a command poisons the mutex; the engine
    /// is still usable afterwards, so the poison is ignored.
    fn lock(&self) -> MutexGuard<'_, Engine> {
        crate::command_hooks::read_allowed();
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs a mutating operation under the lock, then runs any due `on_change`
    /// callbacks after the lock is released (so they may call back into the handle).
    pub(crate) fn mutate<R>(&self, f: impl FnOnce(&mut Engine) -> R) -> R {
        crate::command_hooks::write_allowed();
        let (result, wakers, changed, hooks) = {
            let mut engine = self.lock();
            let before = engine.event_cursor();
            let design_before = engine.design_stamp();
            let result = f(&mut engine);
            if engine.design_stamp() != design_before {
                self.automatic_drc.design_changed();
            }
            let changed = engine.event_cursor() != before;
            (result, engine.take_due_wakers(), changed, engine.take_pending_command_hooks())
        };
        if changed {
            self.session_changed.notify_all();
        }
        for hook in hooks {
            crate::command_hooks::dispatch_or_defer(hook);
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

    /// Read the current event cursor without cloning document contents.
    pub fn event_cursor(&self) -> crate::EventCursor {
        self.lock().event_cursor()
    }

    /// Atomically reads project, selection and the cursor for subsequent changes.
    pub fn session_state(&self) -> crate::SessionSnapshot {
        self.lock().session_state()
    }

    /// Read session changes, waiting at most 30 seconds when no records are available.
    /// The lock is released while waiting; use a blocking worker from async code.
    pub fn watch_events(&self, input: crate::WatchEvents) -> Result<crate::EventBatch, crate::SessionError> {
        crate::command_hooks::check_write().map_err(|_| crate::SessionError::InvalidInput("waiting inside command hooks is unavailable"))?;
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
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.set_selection(input, origin))
    }

    /// Runs the command called `name` with JSON `input` as one transaction.
    pub fn execute(&self, name: &str, input: Value, origin: Origin) -> Result<ExecResult, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.execute(name, input, origin))
    }

    /// Runs a typed command as one transaction.
    pub fn execute_typed<C: Command>(&self, command: C, origin: Origin) -> Result<ExecResult, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.execute_typed(command, origin))
    }

    /// Runs `calls` as one all-or-nothing transaction (see [`Engine::execute_batch`]).
    pub fn execute_batch(&self, calls: Vec<CommandCall>, origin: Origin, label: Option<&str>) -> Result<ExecResult, EngineError> {
        crate::command_hooks::check_write()?;
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
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.transaction(origin, label, f))
    }

    /// Undoes the most recent transaction, whoever made it.
    pub fn undo(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.undo(origin))
    }

    /// Redoes the most recently undone transaction.
    pub fn redo(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.redo(origin))
    }

    /// Undoes `origin`'s most recent transaction, leaving later changes by others
    /// in place (see [`Engine::undo_own`]). This is the agent-facing undo.
    pub fn undo_own(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        crate::command_hooks::check_write()?;
        self.mutate(|engine| engine.undo_own(origin))
    }

    /// Redoes the transaction `origin` most recently undid (see [`Engine::redo_own`]).
    pub fn redo_own(&self, origin: Origin) -> Result<Option<TxId>, EngineError> {
        crate::command_hooks::check_write()?;
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
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        self.read(|project| query.run(project))
    }

    /// Read a consistent named-net summary without changing design, selection or history.
    pub fn query_nets(&self, query: &crate::NetQuery) -> Result<crate::NetQueryResult, crate::CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        self.read(|project| query.run(project))
    }

    /// Read a consistent named-net membership and outstanding guide page.
    pub fn query_connectivity(&self, query: &crate::ConnectivityQuery) -> Result<crate::ConnectivityResult, crate::CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
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
        crate::command_hooks::write_allowed();
        self.lock().subscribe()
    }

    /// Registers `f` to run after every change, outside the engine lock. Meant
    /// as a cheap waker, e.g. egui's `ctx.request_repaint()`; read the details
    /// from a [`subscribe`](EngineHandle::subscribe) receiver.
    pub fn on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        crate::command_hooks::write_allowed();
        self.lock().on_change(f);
    }
}

#[cfg(test)]
mod save_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn native_publication_waits_for_in_flight_save_and_keeps_commands_available() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("save-as.csv");
        let engine = EngineHandle::new(Project::new("save lifecycle"));
        let (writing, written) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let save_engine = engine.clone();
        let save_path = path.clone();
        let saving = std::thread::spawn(move || {
            save_engine
                .save_project_with(crate::SaveProject { path: Some(save_path), overwrite: false }, |project, path, _| {
                    viamind_doc::file::save_new(project, path)?;
                    writing.send(()).unwrap();
                    released.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        });
        written.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(engine.saves.try_lock().is_err(), "save owns the full native lifecycle");
        assert!(engine.document_status().path.is_none(), "save has written but not marked the active path yet");
        let publication_engine = engine.clone();
        let publication = std::thread::spawn(move || publication_engine.with_native_file_guard(|current| current.map(Path::to_owned)));
        engine.execute("add_part", serde_json::json!({"designator":"R1","value":"","footprint":"F","x_mm":0,"y_mm":0}), Origin::User).unwrap();
        release.send(()).unwrap();
        saving.join().unwrap();
        assert_eq!(publication.join().unwrap(), Some(path));
        engine.with_native_file_guard(|_| {
            assert!(engine.saves.try_lock().is_err(), "publication excludes native open/new/save");
            // Ordinary edits remain possible during the small publication section.
            engine.execute("add_part", serde_json::json!({"designator":"R2","value":"","footprint":"F","x_mm":0,"y_mm":0}), Origin::User).unwrap();
        });
        assert_eq!(engine.read(|p| p.board.parts.len()), 2);
    }

    #[test]
    fn concurrent_edit_during_open_rejects_the_loaded_document() {
        let engine = EngineHandle::new(Project::new("Keep"));
        let result = engine.replace_project("open_project", Default::default(), Origin::User, || {
            engine.execute(
                "add_part",
                serde_json::json!({ "designator": "R1", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 }),
                Origin::agent("writer"),
            )?;
            Ok((Project::new("Should not load"), None))
        });
        assert!(result.unwrap_err().to_string().contains("document changed"));
        assert_eq!(engine.read(|p| p.name.clone()), "Keep");
        assert_eq!(engine.read(|p| p.board.parts.len()), 1);
        assert!(engine.document_status().dirty);
        assert!(engine.can_undo());
    }

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
