//! Shared, transient automatic scoped DRC. The worker never owns the engine.
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::Project;
use viamind_drc::{DrcInput, DrcReport};

use crate::auto_drc_events::{AutoDrcEventBatch, AutoDrcEventError, AutoDrcEventState, AutoDrcJournal, WatchAutoDrcEvents};
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
    events: AutoDrcJournal,
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
    pub(crate) fn event_state(&self) -> AutoDrcEventState {
        let state = self.shared.lock();
        AutoDrcEventState { status: state.status.clone(), cursor: state.events.cursor() }
    }
    pub(crate) fn watch_events(&self, input: WatchAutoDrcEvents) -> Result<AutoDrcEventBatch, AutoDrcEventError> {
        input.validate()?;
        let deadline = Instant::now() + Duration::from_millis(input.timeout_ms);
        let mut state = self.shared.lock();
        let after = input.after.unwrap_or_else(|| state.events.cursor());
        loop {
            // Revalidate the retention floor after every wake, including spurious wakes.
            let batch = state.events.read(after, input.limit)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !batch.events.is_empty() || remaining.is_zero() {
                return Ok(batch);
            }
            state = self.shared.changed.wait_timeout(state, remaining).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
    fn record(state: &mut State) {
        state.events.push(&state.status);
    }
    /// Caller holds engine lock; all mutations use engine -> state lock order.
    pub(crate) fn configure(&self, config: AutoDrcConfig) -> Result<AutoDrcStatus, EngineError> {
        self.configure_with_spawn(config, |engine, shared, checker| {
            std::thread::Builder::new().name("viamind-auto-drc".into()).spawn(move || worker(engine, shared, checker)).map(|_| ())
        })
    }
    fn configure_with_spawn(
        &self,
        config: AutoDrcConfig,
        spawn: impl FnOnce(Weak<Mutex<Engine>>, Arc<Shared>, Arc<Checker>) -> std::io::Result<()>,
    ) -> Result<AutoDrcStatus, EngineError> {
        config.validate()?;
        let mut state = self.shared.lock();
        state.status.config = config;
        Self::invalidate(&mut state);
        Self::record(&mut state);
        if state.status.config.enabled && !state.started {
            let shared = self.shared.clone();
            let engine = self.engine.clone();
            let checker = self.checker.clone();
            spawn(engine, shared, checker).map_err(|e| {
                state.pending = None;
                state.status.phase = AutoDrcPhase::Failed;
                state.status.message = Some(format!("Could not start automatic DRC worker: {e}"));
                Self::record(&mut state);
                self.shared.changed.notify_all();
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
            Self::record(&mut state);
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
            AutoDrcController::record(&mut state);
            shared.changed.notify_all();
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
                board: engine.project().board.clone_for_electrical_analysis(),
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
        AutoDrcController::record(&mut state);
        shared.changed.notify_all();
    }
}
/// Check cheap lengths before cloning. Iterate only after structural counts fit.
pub(crate) fn snapshot_budget(project: &Project, limit: usize) -> Result<(), String> {
    snapshot_budget_with_text_growth(project, limit, 0)
}
/// Reserve a replacement's positive net-text growth before cloning bounded geometry.
pub(crate) fn snapshot_budget_with_text_growth(project: &Project, limit: usize, growth: usize) -> Result<(), String> {
    let board = &project.board;
    let mut objects = board.parts.len().saturating_add(board.tracks.len()).saturating_add(board.vias.len());
    let policy = viamind_doc::named_net_rule_budget(&board.net_rules).map_err(|e| format!("Automatic DRC skipped: {e}"))?;
    objects = objects.saturating_add(policy.rows);
    let mut text = policy.text_bytes.saturating_add(growth);
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

    #[test]
    fn spawn_failure_records_and_wakes_without_touching_document() {
        let handle = EngineHandle::new(Project::new("failed spawn"));
        let before = handle.session_state();
        let baseline = handle.auto_drc_event_state();
        let error = handle.automatic_drc.configure_with_spawn(enabled(), |_, _, _| Err(std::io::Error::other("injected spawn failure"))).unwrap_err();
        assert!(error.to_string().contains("injected spawn failure"));
        let events = handle.watch_auto_drc_events(crate::WatchAutoDrcEvents { after: Some(baseline.cursor), ..Default::default() }).unwrap();
        assert_eq!(events.events.iter().map(|event| event.phase).collect::<Vec<_>>(), vec![AutoDrcPhase::Pending, AutoDrcPhase::Failed]);
        assert_eq!(events.events[0].generation, 1);
        assert!(events.events[1].message.as_ref().unwrap().contains("injected spawn failure"));
        assert_eq!(handle.session_state(), before);
        assert!(!handle.automatic_drc.shared.lock().started);
        handle.configure_auto_drc(enabled()).unwrap();
        assert_eq!(wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready).generation, 2);
    }
    #[test]
    fn independent_observers_timeout_and_selection_keep_document_guards() {
        let handle = EngineHandle::new(Project::new("independent observers"));
        add(&handle, "R1");
        let before = handle.session_state();
        let baseline = handle.auto_drc_event_state();
        assert_eq!(baseline.cursor.sequence, 0);
        assert_eq!(baseline.status, handle.auto_drc_status());
        let watcher = handle.clone();
        let tail = baseline.cursor;
        let observing = std::thread::spawn(move || {
            watcher.watch_auto_drc_events(crate::WatchAutoDrcEvents { after: Some(tail), timeout_ms: 5000, ..Default::default() }).unwrap()
        });
        handle.configure_auto_drc(enabled()).unwrap();
        let first = observing.join().unwrap();
        assert!(!first.events.is_empty());
        assert_eq!(first.events[0].phase, AutoDrcPhase::Pending);
        wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready);
        let all = handle.watch_auto_drc_events(crate::WatchAutoDrcEvents { after: Some(tail), ..Default::default() }).unwrap();
        assert_eq!(all.events.iter().map(|e| e.phase).collect::<Vec<_>>(), vec![AutoDrcPhase::Pending, AutoDrcPhase::Running, AutoDrcPhase::Ready]);
        assert_eq!(handle.session_state(), before);
        let current = handle.auto_drc_event_state();
        assert_eq!(current.cursor, all.cursor);
        assert_eq!(all.events[2].source_cursor, current.status.source_cursor);
        let time = Instant::now();
        let empty =
            handle.watch_auto_drc_events(crate::WatchAutoDrcEvents { after: Some(all.cursor), timeout_ms: 20, ..Default::default() }).unwrap();
        assert!(empty.events.is_empty());
        assert_eq!(empty.cursor, all.cursor);
        assert!(time.elapsed() >= Duration::from_millis(20));
        handle
            .set_selection(SetSelection { ids: before.project.board.parts.keys().copied().collect(), mode: Default::default() }, Origin::User)
            .unwrap();
        assert_eq!(handle.auto_drc_event_state(), current);
        assert_ne!(handle.event_cursor(), before.cursor);
        let wrong = crate::AutoDrcEventCursor { stream_id: before.cursor.stream_id, sequence: 0 };
        assert_eq!(
            handle.watch_auto_drc_events(crate::WatchAutoDrcEvents { after: Some(wrong), ..Default::default() }).unwrap_err(),
            crate::AutoDrcEventError::WrongStream
        );
        for input in [
            crate::WatchAutoDrcEvents { limit: 0, ..Default::default() },
            crate::WatchAutoDrcEvents { limit: 129, ..Default::default() },
            crate::WatchAutoDrcEvents { timeout_ms: 30001, ..Default::default() },
        ] {
            assert!(handle.watch_auto_drc_events(input).is_err());
        }
        let journal = handle.auto_drc_event_state();
        assert!(handle.configure_auto_drc(AutoDrcConfig { max_objects: 0, ..Default::default() }).is_err());
        assert_eq!(handle.auto_drc_event_state(), journal);
        handle.configure_auto_drc(Default::default()).unwrap();
        let disabled = handle.auto_drc_event_state();
        add(&handle, "R2");
        handle
            .new_project(
                NewProject { name: "disabled replacement".into(), discard_dirty: true, expected_cursor: Some(handle.event_cursor()) },
                Origin::User,
            )
            .unwrap();
        assert_eq!(handle.auto_drc_event_state(), disabled);
    }
    fn enabled() -> AutoDrcConfig {
        AutoDrcConfig { enabled: true, debounce_ms: 0, ..Default::default() }
    }
    #[test]
    fn worker_snapshot_excludes_invalid_unused_sourcing() {
        let mut project = Project::new("bounded geometry");
        crate::Command::apply(
            crate::AddPart {
                expected_project: None,
                designator: "R1".into(),
                value: String::new(),
                footprint: "legacy".into(),
                x_mm: 0.0,
                y_mm: 0.0,
                rotation_deg: None,
                side: None,
            },
            &mut project,
        )
        .unwrap();
        for part in project.board.parts.values_mut() {
            part.assembly = Some(Box::new(viamind_doc::AssemblyMetadata { mpn: Some("unused".repeat(400000)), ..Default::default() }));
        }
        let mut handle = EngineHandle::new(project);
        let (tx, rx) = mpsc::channel();
        Arc::get_mut(&mut handle.automatic_drc).unwrap().checker = Arc::new(move |project, rules| {
            assert!(project.board.parts.values().all(|p| p.assembly.is_none()));
            tx.send(()).unwrap();
            viamind_drc::run_drc(project, rules)
        });
        handle.configure_auto_drc(enabled()).unwrap();
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        wait_status(&handle, |s| s.phase == AutoDrcPhase::Ready);
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
        let events = handle
            .watch_auto_drc_events(crate::WatchAutoDrcEvents {
                after: Some(crate::AutoDrcEventCursor { stream_id: handle.auto_drc_event_state().cursor.stream_id, sequence: 0 }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            events.events.iter().map(|e| (e.generation, e.phase)).collect::<Vec<_>>(),
            vec![
                (1, AutoDrcPhase::Pending),
                (1, AutoDrcPhase::Running),
                (2, AutoDrcPhase::Pending),
                (3, AutoDrcPhase::Pending),
                (4, AutoDrcPhase::Pending),
                (4, AutoDrcPhase::Running),
                (4, AutoDrcPhase::Ready),
            ]
        );
        assert_eq!(events.events.last().unwrap().report.as_ref().unwrap().excluded_part_count, 3);
        assert!(events.events[..6].iter().all(|event| event.report.is_none() && event.source_cursor.is_none()));
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
        let events = all_events(&handle);
        assert_eq!(events.iter().filter(|event| event.phase == AutoDrcPhase::Ready).map(|event| event.generation).collect::<Vec<_>>(), vec![3]);
        assert!(events.iter().filter_map(|event| event.source_cursor).all(|cursor| cursor.stream_id != old_stream));
    }
    #[test]
    fn selection_during_blocked_check_keeps_original_capture_cursor() {
        let (handle, entered, release) = blocked();
        add(&handle, "R1");
        let capture = handle.session_state();
        handle.configure_auto_drc(enabled()).unwrap();
        entered.recv_timeout(Duration::from_secs(10)).unwrap();
        handle
            .set_selection(SetSelection { ids: capture.project.board.parts.keys().copied().collect(), mode: Default::default() }, Origin::User)
            .unwrap();
        let selected = handle.session_state();
        assert_ne!(selected.cursor, capture.cursor);
        release.send(()).unwrap();
        let ready = wait_status(&handle, |status| status.phase == AutoDrcPhase::Ready);
        assert_eq!(ready.source_cursor, Some(capture.cursor));
        assert_eq!(ready.generation, 1);
        assert_eq!(handle.session_state(), selected);
        let events = all_events(&handle);
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].source_cursor, Some(capture.cursor));
    }
    fn all_events(handle: &EngineHandle) -> Vec<crate::AutoDrcEvent> {
        handle
            .watch_auto_drc_events(crate::WatchAutoDrcEvents {
                after: Some(crate::AutoDrcEventCursor { stream_id: handle.auto_drc_event_state().cursor.stream_id, sequence: 0 }),
                limit: 128,
                ..Default::default()
            })
            .unwrap()
            .events
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
        let skipped_event = all_events(&handle).pop().unwrap();
        assert_eq!(skipped_event.phase, AutoDrcPhase::Skipped);
        assert!(skipped_event.report.is_none());
        assert!(skipped_event.message.unwrap().contains("exceeds 1 structural objects"));
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
        let failure = all_events(&handle).pop().unwrap();
        assert_eq!(failure.phase, AutoDrcPhase::Failed);
        assert!(failure.report.is_none());
        assert!(failure.message.unwrap().contains("panicked"));
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
