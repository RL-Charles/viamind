//! Shared, transient automatic scoped DRC. The worker never owns the engine.
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::Project;
use viamind_drc::{DrcInput, DrcReport};

use crate::{Engine, EngineError, EventCursor};

pub const AUTO_DRC_MAX_OBJECTS: usize = 10_000;
pub const AUTO_DRC_MAX_LABEL_BYTES: usize = 512;
pub const AUTO_DRC_MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AutoDrcConfig {
    pub enabled: bool,
    pub rules: DrcInput,
    #[schemars(range(min = 0, max = 2000))]
    pub debounce_ms: u64,
    #[schemars(range(min = 1, max = 10000))]
    pub max_objects: usize,
}
impl Default for AutoDrcConfig {
    fn default() -> Self {
        Self { enabled: false, rules: DrcInput::default(), debounce_ms: 20, max_objects: 5000 }
    }
}
impl AutoDrcConfig {
    fn validate(&self) -> Result<(), EngineError> {
        let invalid = |message: String| EngineError::InvalidInput { command: "configure_auto_drc".into(), message };
        self.rules.validate().map_err(|e| invalid(e.to_string()))?;
        if self.debounce_ms > 2000 || !(1..=AUTO_DRC_MAX_OBJECTS).contains(&self.max_objects) {
            return Err(invalid("debounce_ms must be 0..2000 and max_objects 1..10000".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoDrcPhase {
    Disabled,
    Pending,
    Running,
    Ready,
    Skipped,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoDrcStatus {
    pub config: AutoDrcConfig,
    pub generation: u64,
    pub phase: AutoDrcPhase,
    pub source_cursor: Option<EventCursor>,
    /// Present only for a current ready generation; includes all scoped exclusions.
    pub report: Option<DrcReport>,
    pub message: Option<String>,
    pub check_ms: Option<f64>,
    pub snapshot_capture_ms: Option<f64>,
    /// Latest request to publication, not native desktop edit-to-visible latency.
    pub request_to_publication_ms: Option<f64>,
    pub max_snapshot_text_bytes: usize,
    pub max_label_bytes: usize,
}
impl Default for AutoDrcStatus {
    fn default() -> Self {
        Self {
            config: AutoDrcConfig::default(),
            generation: 0,
            phase: AutoDrcPhase::Disabled,
            source_cursor: None,
            report: None,
            message: None,
            check_ms: None,
            snapshot_capture_ms: None,
            request_to_publication_ms: None,
            max_snapshot_text_bytes: AUTO_DRC_MAX_TEXT_BYTES,
            max_label_bytes: AUTO_DRC_MAX_LABEL_BYTES,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignStamp {
    pub cursor_stream: viamind_doc::ObjectId,
    pub revision: u64,
}
#[derive(Debug)]
struct Request {
    generation: u64,
    requested: Instant,
    deadline: Instant,
}
#[derive(Debug, Default)]
struct State {
    status: AutoDrcStatus,
    pending: Option<Request>,
    started: bool,
    stop: bool,
}
#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
type Checker = dyn Fn(&Project, &DrcInput) -> Result<DrcReport, viamind_drc::DrcError> + Send + Sync;

pub(crate) struct AutoDrcController {
    engine: Weak<Mutex<Engine>>,
    shared: Arc<Shared>,
    checker: Arc<Checker>,
}
impl std::fmt::Debug for AutoDrcController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoDrcController").finish_non_exhaustive()
    }
}
impl Drop for AutoDrcController {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.changed.notify_all();
    }
}
impl AutoDrcController {
    pub(crate) fn new(engine: &Arc<Mutex<Engine>>) -> Self {
        Self { engine: Arc::downgrade(engine), shared: Arc::default(), checker: Arc::new(viamind_drc::run_drc) }
    }
    pub(crate) fn status(&self) -> AutoDrcStatus {
        self.shared.lock().status.clone()
    }
    /// Caller holds engine lock; all mutations use engine -> state lock order.
    pub(crate) fn configure(&self, config: AutoDrcConfig) -> Result<AutoDrcStatus, EngineError> {
        config.validate()?;
        let mut state = self.shared.lock();
        state.status.config = config;
        Self::invalidate(&mut state);
        if state.status.config.enabled && !state.started {
            let shared = self.shared.clone();
            let engine = self.engine.clone();
            let checker = self.checker.clone();
            std::thread::Builder::new().name("viamind-auto-drc".into()).spawn(move || worker(engine, shared, checker)).map_err(|e| {
                state.pending = None;
                state.status.phase = AutoDrcPhase::Failed;
                state.status.message = Some(format!("Could not start automatic DRC worker: {e}"));
                EngineError::InvalidInput { command: "configure_auto_drc".into(), message: e.to_string() }
            })?;
            state.started = true;
        }
        self.shared.changed.notify_all();
        Ok(state.status.clone())
    }
    pub(crate) fn design_changed(&self) {
        let mut state = self.shared.lock();
        if state.status.config.enabled {
            Self::invalidate(&mut state);
            self.shared.changed.notify_all();
        }
    }
    fn invalidate(state: &mut State) {
        state.status.generation = state.status.generation.checked_add(1).expect("automatic DRC generation exhausted");
        state.status.report = None;
        state.status.source_cursor = None;
        state.status.message = None;
        state.status.check_ms = None;
        state.status.snapshot_capture_ms = None;
        state.status.request_to_publication_ms = None;
        if state.status.config.enabled {
            let now = Instant::now();
            state.pending = Some(Request {
                generation: state.status.generation,
                requested: now,
                deadline: now + Duration::from_millis(state.status.config.debounce_ms),
            });
            state.status.phase = AutoDrcPhase::Pending;
        } else {
            state.pending = None;
            state.status.phase = AutoDrcPhase::Disabled;
        }
    }
}
fn worker(engine: Weak<Mutex<Engine>>, shared: Arc<Shared>, checker: Arc<Checker>) {
    loop {
        let (request, config) = {
            let mut state = shared.lock();
            loop {
                if state.stop {
                    return;
                }
                if let Some(request) = &state.pending {
                    let now = Instant::now();
                    if now >= request.deadline {
                        break;
                    }
                    let wait = request.deadline - now;
                    state = shared.changed.wait_timeout(state, wait).unwrap_or_else(PoisonError::into_inner).0;
                } else {
                    state = shared.changed.wait(state).unwrap_or_else(PoisonError::into_inner);
                }
            }
            let request = state.pending.take().expect("pending request");
            state.status.phase = AutoDrcPhase::Running;
            (request, state.status.config.clone())
        };
        let Some(owner) = engine.upgrade() else { return };
        let (stamp, cursor, snapshot, capture_ms) = {
            let engine = owner.lock().unwrap_or_else(PoisonError::into_inner);
            {
                let state = shared.lock();
                if state.stop {
                    return;
                }
                if state.status.generation != request.generation {
                    continue;
                }
            }
            let capture_started = Instant::now();
            let snapshot = snapshot_budget(engine.project(), config.max_objects).map(|()| Project {
                id: engine.project().id,
                name: String::new(),
                schematic: Default::default(),
                board: engine.project().board.clone(),
            });
            (engine.design_stamp(), engine.event_cursor(), snapshot, capture_started.elapsed().as_secs_f64() * 1000.0)
        };
        drop(owner); // No engine ownership or lock is retained while checking.
        let started = Instant::now();
        let result = snapshot.map(|project| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| checker(&project, &config.rules)))
                .unwrap_or_else(|_| Err(viamind_drc::DrcError("automatic checker panicked; no report was published".into())))
        });
        let check_ms = started.elapsed().as_secs_f64() * 1000.0;
        let Some(owner) = engine.upgrade() else { return };
        let engine = owner.lock().unwrap_or_else(PoisonError::into_inner);
        let mut state = shared.lock();
        if state.stop {
            return;
        }
        if state.status.generation != request.generation || engine.design_stamp() != stamp {
            continue;
        }
        state.status.source_cursor = Some(cursor);
        state.status.check_ms = Some(check_ms);
        state.status.snapshot_capture_ms = Some(capture_ms);
        state.status.request_to_publication_ms = Some(request.requested.elapsed().as_secs_f64() * 1000.0);
        match result {
            Ok(Ok(report)) => {
                state.status.phase = AutoDrcPhase::Ready;
                state.status.report = Some(report);
            }
            Ok(Err(error)) => {
                state.status.phase = AutoDrcPhase::Failed;
                state.status.message = Some(error.to_string());
            }
            Err(reason) => {
                state.status.phase = AutoDrcPhase::Skipped;
                state.status.message = Some(reason);
            }
        }
        shared.changed.notify_all();
    }
}
/// Check cheap lengths before cloning. Iterate only after structural counts fit.
pub(crate) fn snapshot_budget(project: &Project, limit: usize) -> Result<(), String> {
    let board = &project.board;
    let mut objects = board.parts.len().saturating_add(board.tracks.len()).saturating_add(board.vias.len());
    let mut text = 0_usize;
    let check = |objects: usize, text: usize| {
        if objects > limit {
            Err(format!("Automatic DRC skipped: snapshot exceeds {limit} structural objects; use batch DRC or raise the bounded limit"))
        } else if text > AUTO_DRC_MAX_TEXT_BYTES {
            Err(format!("Automatic DRC skipped: snapshot text exceeds {AUTO_DRC_MAX_TEXT_BYTES} bytes"))
        } else {
            Ok(())
        }
    };
    check(objects, text)?;
    if let Some(outline) = &board.outline {
        // Ring records count too, preventing iteration of arbitrarily many empty rings.
        objects = objects.saturating_add(outline.cutouts.len());
        check(objects, text)?;
        for ring in outline.rings() {
            objects = objects.saturating_add(ring.points.len());
            check(objects, text)?;
        }
    }
    let label = |value: &str| {
        if value.len() > AUTO_DRC_MAX_LABEL_BYTES {
            Err(format!("Automatic DRC skipped: a designator, pad number or net exceeds {AUTO_DRC_MAX_LABEL_BYTES} label bytes"))
        } else {
            Ok(())
        }
    };
    for part in board.parts.values() {
        label(&part.designator)?;
        text = text.saturating_add(part.designator.len()).saturating_add(part.value.len()).saturating_add(part.footprint.len());
        check(objects, text)?;
        if let Some(pads) = &part.pads {
            objects = objects.saturating_add(pads.0.len());
            check(objects, text)?;
            for pad in &pads.0 {
                label(&pad.number)?;
                if let Some(net) = &pad.net {
                    label(net)?;
                }
                text = text.saturating_add(pad.number.len()).saturating_add(pad.net.as_ref().map_or(0, String::len));
                check(objects, text)?;
            }
        }
    }
    for net in board.tracks.values().map(|t| &t.net).chain(board.vias.values().map(|v| &v.net)) {
        if let Some(net) = net {
            label(net)?;
        }
        text = text.saturating_add(net.as_ref().map_or(0, String::len));
        check(objects, text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineHandle, NewProject, Origin, SetSelection};
    use serde_json::json;
    use std::sync::mpsc;

    fn enabled() -> AutoDrcConfig {
        AutoDrcConfig { enabled: true, debounce_ms: 0, ..Default::default() }
    }
    fn wait_status(handle: &EngineHandle, predicate: impl Fn(&AutoDrcStatus) -> bool) -> AutoDrcStatus {
        let shared = &handle.automatic_drc.shared;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut state = shared.lock();
        loop {
            if predicate(&state.status) {
                return state.status.clone();
            }
            let remaining = deadline.checked_duration_since(Instant::now()).expect("worker status timeout");
            state = shared.changed.wait_timeout(state, remaining).unwrap().0;
        }
    }
    fn add(handle: &EngineHandle, designator: &str) {
        handle.execute("add_part", json!({"designator":designator,"value":"test","footprint":"legacy","x_mm":0,"y_mm":0}), Origin::User).unwrap();
    }
    /// First invocation blocks deterministically; subsequent checks run normally.
    fn blocked() -> (EngineHandle, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let mut handle = EngineHandle::new(Project::new("blocked"));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(Some(release_rx));
        Arc::get_mut(&mut handle.automatic_drc).unwrap().checker = Arc::new(move |project, rules| {
            let release = release_rx.lock().unwrap().take();
            if let Some(release) = release {
                entered_tx.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            viamind_drc::run_drc(project, rules)
        });
        (handle, entered_rx, release_tx)
    }
    #[test]
    fn blocked_worker_does_not_block_edits_coalesces_and_rejects_old_result() {
        let (handle, entered, release) = blocked();
        let before = handle.session_state();
        handle.configure_auto_drc(enabled()).unwrap();
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(handle.session_state(), before);
        // Would deadlock here if the checker retained the engine mutex.
        for name in ["A", "B", "C"] {
            add(&handle, name);
        }
        let pending = handle.auto_drc_status();
        assert_eq!(pending.phase, AutoDrcPhase::Pending);
        assert!(pending.report.is_none());
        assert_eq!(pending.generation, 4);
        assert!(handle.automatic_drc.shared.lock().pending.is_some());
        release.send(()).unwrap();
        let ready = wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready);
        assert_eq!(ready.generation, 4);
        assert_eq!(ready.report.unwrap().coverage.excluded_part_count, 3);
        assert_eq!(ready.source_cursor, Some(handle.session_state().cursor));
        assert!(ready.check_ms.unwrap().is_finite());
        assert!(ready.request_to_publication_ms.unwrap() >= ready.check_ms.unwrap());
    }
    #[test]
    fn config_and_project_replacement_supersede_running_snapshot() {
        let (handle, entered, release) = blocked();
        add(&handle, "Old");
        handle.configure_auto_drc(enabled()).unwrap();
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        let old_stream = handle.session_state().cursor.stream_id;
        let mut config = enabled();
        config.rules.copper_clearance_mm = 0.7;
        handle.configure_auto_drc(config.clone()).unwrap();
        let cursor = handle.session_state().cursor;
        handle.new_project(NewProject { name: "replacement".into(), discard_dirty: true, expected_cursor: Some(cursor) }, Origin::User).unwrap();
        release.send(()).unwrap();
        let ready = wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready);
        assert_eq!(ready.generation, 3);
        assert_eq!(ready.config, config);
        assert_ne!(ready.source_cursor.unwrap().stream_id, old_stream);
        assert_eq!(ready.report.unwrap().coverage.excluded_part_count, 0);
    }
    #[test]
    fn disable_and_invalid_config_are_atomic_and_read_only_selection_does_not_rerun() {
        let handle = EngineHandle::new(Project::new("runtime"));
        add(&handle, "A");
        let before = handle.session_state();
        let document = handle.document_status();
        let history = handle.with_engine(|e| (e.undo_history().len(), e.log().entries().len()));
        handle.configure_auto_drc(enabled()).unwrap();
        let ready = wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready);
        for bad in [
            AutoDrcConfig { max_objects: 0, ..enabled() },
            AutoDrcConfig { debounce_ms: 2001, ..enabled() },
            AutoDrcConfig { rules: DrcInput { copper_clearance_mm: f64::NAN, ..Default::default() }, ..enabled() },
        ] {
            assert!(handle.configure_auto_drc(bad).is_err());
            assert_eq!(handle.auto_drc_status(), ready);
        }
        assert_eq!(handle.session_state(), before);
        assert_eq!(handle.document_status(), document);
        assert_eq!(handle.with_engine(|e| (e.undo_history().len(), e.log().entries().len())), history);
        let id = handle.read(|p| *p.board.parts.keys().next().unwrap());
        handle.set_selection(SetSelection { ids: vec![id], mode: Default::default() }, Origin::User).unwrap();
        assert_eq!(handle.auto_drc_status(), ready);
        let directory = tempfile::tempdir().unwrap();
        handle.save_project(crate::SaveProject { path: Some(directory.path().join("automatic.via")), ..Default::default() }).unwrap();
        assert_eq!(handle.auto_drc_status(), ready, "saving must not rerun DRC");
        handle.configure_auto_drc(AutoDrcConfig::default()).unwrap();
        let disabled = handle.auto_drc_status();
        assert_eq!(disabled.phase, AutoDrcPhase::Disabled);
        assert!(disabled.report.is_none());
        let generation = disabled.generation;
        handle.undo(Origin::User).unwrap();
        assert_eq!(handle.auto_drc_status().generation, generation);
    }
    #[test]
    fn bounded_snapshot_skips_without_all_clear_and_can_recover() {
        let handle = EngineHandle::new(Project::new("bounds"));
        add(&handle, "A");
        add(&handle, "B");
        handle.configure_auto_drc(AutoDrcConfig { max_objects: 1, ..enabled() }).unwrap();
        let skipped = wait_status(&handle, |s| s.phase == AutoDrcPhase::Skipped);
        assert!(skipped.report.is_none());
        assert!(skipped.message.unwrap().contains("exceeds 1 structural objects"));
        handle.configure_auto_drc(enabled()).unwrap();
        assert_eq!(wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready).report.unwrap().coverage.excluded_part_count, 2);
        let mut project = Project::new("huge unrelated schematic name");
        let part = handle.read(|p| p.board.parts.values().next().unwrap().clone());
        let mut huge = part;
        huge.value = "x".repeat(AUTO_DRC_MAX_TEXT_BYTES + 1);
        project.board.parts.insert(huge.id, huge);
        assert!(snapshot_budget(&project, 5000).unwrap_err().contains("text exceeds"));
        let part = project.board.parts.values_mut().next().unwrap();
        part.value.clear();
        part.designator = "x".repeat(AUTO_DRC_MAX_LABEL_BYTES + 1);
        assert!(snapshot_budget(&project, 5000).unwrap_err().contains("label bytes"));
    }
    #[test]
    fn disabled_running_generation_cannot_publish_and_checker_failure_can_recover() {
        let (handle, entered, release) = blocked();
        handle.configure_auto_drc(enabled()).unwrap();
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.configure_auto_drc(AutoDrcConfig::default()).unwrap();
        assert_eq!(handle.auto_drc_status().phase, AutoDrcPhase::Disabled);
        assert!(handle.auto_drc_status().report.is_none());
        handle.configure_auto_drc(enabled()).unwrap();
        release.send(()).unwrap();
        let ready = wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready);
        assert_eq!(ready.generation, 3);

        let mut handle = EngineHandle::new(Project::new("checker failure"));
        let fail_once = std::sync::atomic::AtomicBool::new(true);
        Arc::get_mut(&mut handle.automatic_drc).unwrap().checker = Arc::new(move |project, rules| {
            assert!(!fail_once.swap(false, std::sync::atomic::Ordering::SeqCst), "injected checker panic");
            viamind_drc::run_drc(project, rules)
        });
        handle.configure_auto_drc(enabled()).unwrap();
        let failed = wait_status(&handle, |s| s.phase == AutoDrcPhase::Failed);
        assert!(failed.report.is_none());
        assert!(failed.message.unwrap().contains("panicked"));
        handle.configure_auto_drc(enabled()).unwrap();
        assert_eq!(wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready).generation, 2);
    }
    #[test]
    fn last_handle_drop_stops_worker_without_retaining_engine() {
        let (handle, entered, release) = blocked();
        let weak_engine = Arc::downgrade(&handle.engine);
        let weak_shared = Arc::downgrade(&handle.automatic_drc.shared);
        handle.configure_auto_drc(enabled()).unwrap();
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(handle);
        assert!(weak_engine.upgrade().is_none(), "checker retained engine ownership");
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while weak_shared.upgrade().is_some() {
            assert!(Instant::now() < deadline, "worker retained runtime state");
            std::thread::yield_now();
        }
    }
}
