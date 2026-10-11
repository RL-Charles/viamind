//! Trusted, transient command validators and bounded commit observations.
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError};

use crate::{ChangeKind, Delta, EngineError, EventCursor, Origin, TxId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project};
use viamind_drc::DrcInput;

const BUILTIN: &str = "builtin_drc";
const MAX_TEXT: usize = 1024;
const MAX_PENDING: usize = 32;
thread_local! {
    static PHASE: Cell<Option<HookPhase>> = const { Cell::new(None) };
    static NATIVE_DEPTH: Cell<usize> = const { Cell::new(0) };
    static NATIVE_PENDING: RefCell<Vec<PostCommandDispatch>> = const { RefCell::new(Vec::new()) };
}
/// Defers observations from edits within a native publication closure until its
/// outer native mutex is released. Construct before taking that mutex.
pub(crate) struct NativeHookScope;
impl NativeHookScope {
    pub(crate) fn enter() -> Self {
        NATIVE_DEPTH.set(NATIVE_DEPTH.get() + 1);
        Self
    }
}
impl Drop for NativeHookScope {
    fn drop(&mut self) {
        let depth = NATIVE_DEPTH.get() - 1;
        NATIVE_DEPTH.set(depth);
        if depth == 0 {
            let pending = NATIVE_PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
            for dispatch in pending {
                dispatch.run();
            }
        }
    }
}
pub(crate) fn dispatch_or_defer(dispatch: PostCommandDispatch) {
    if NATIVE_DEPTH.get() != 0 {
        NATIVE_PENDING.with(|p| p.borrow_mut().push(dispatch));
    } else {
        dispatch.run();
    }
}
fn native_pending_count() -> usize {
    NATIVE_PENDING.with(|p| p.borrow().len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    Pre,
    Post,
}
struct PhaseGuard(Option<HookPhase>);
impl PhaseGuard {
    fn enter(phase: HookPhase) -> Self {
        Self(PHASE.replace(Some(phase)))
    }
}
impl Drop for PhaseGuard {
    fn drop(&mut self) {
        PHASE.set(self.0);
    }
}
pub(crate) fn check_read() -> Result<(), EngineError> {
    if PHASE.get() == Some(HookPhase::Pre) { Err(invalid("captured handle reads are unavailable inside pre-command hooks; use borrowed candidate")) } else { Ok(()) }
}
pub(crate) fn read_allowed() {
    if PHASE.get() == Some(HookPhase::Pre) {
        std::panic::panic_any(Reentry);
    }
}
pub(crate) fn write_allowed() {
    if PHASE.get().is_some() {
        std::panic::panic_any(Reentry);
    }
}
pub(crate) fn check_write() -> Result<(), EngineError> {
    if PHASE.get().is_some() { Err(invalid("captured handle mutation/wait is unavailable inside command hooks")) } else { Ok(()) }
}
#[derive(Debug)]
struct Reentry;
fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::InvalidInput { command: "command_hooks".into(), message: message.into() }
}
fn bounded(text: &str) -> (String, bool) {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), end != text.len())
}
fn invoke(phase: HookPhase, call: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    let _phase = PhaseGuard::enter(phase);
    match catch_unwind(AssertUnwindSafe(call)) {
        Ok(result) => result,
        Err(payload) => {
            let message = if payload.is::<Reentry>() {
                "captured handle re-entry refused".to_owned()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                bounded(s).0
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                bounded(s).0
            } else {
                "callback panicked with a non-string payload".to_owned()
            };
            // An arbitrary panic payload's destructor can panic again. Trusted
            // unknown payloads are deliberately leaked instead of risking unwind.
            if !payload.is::<String>() && !payload.is::<&str>() && !payload.is::<Reentry>() {
                std::mem::forget(payload);
            }
            Err(format!("callback panic: {message}"))
        }
    }
}

/// Borrowed final candidate, after coalescing and before transaction publication.
pub struct PreCommand<'a> {
    pub project: &'a Project,
    pub deltas: &'a [Delta],
    pub source_cursor: EventCursor,
    pub origin: &'a Origin,
    pub label: &'a str,
    pub kind: ChangeKind,
}
pub type PreCommandHook = Arc<dyn for<'a> Fn(&PreCommand<'a>) -> Result<(), String> + Send + Sync>;
pub type PostCommandHook = Arc<dyn Fn(&PostCommand) -> Result<(), String> + Send + Sync>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DeltaCounts {
    pub parts: usize,
    pub sheets: usize,
    pub outlines: usize,
    pub tracks: usize,
    pub vias: usize,
    pub named_net_rules: usize,
}
impl DeltaCounts {
    fn from_deltas(deltas: &[Delta]) -> Self {
        let mut c = Self::default();
        for d in deltas {
            match d {
                Delta::Part { .. } => c.parts += 1,
                Delta::Sheet { .. } => c.sheets += 1,
                Delta::BoardOutline { .. } => c.outlines += 1,
                Delta::Track { .. } => c.tracks += 1,
                Delta::Via { .. } => c.vias += 1,
                Delta::NamedNetRules { .. } => c.named_net_rules += 1,
            }
        }
        c
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PostCommand {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub committed_cursor: EventCursor,
    pub tx: TxId,
    pub origin: Origin,
    pub label: String,
    pub kind: ChangeKind,
    pub configuration_generation: u64,
    pub delta_counts: DeltaCounts,
    pub metadata_abbreviated: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CommandHookConfig {
    pub enabled: bool,
    pub rules: DrcInput,
    #[schemars(range(min = 1, max = 10000))]
    pub max_objects: usize,
}
impl Default for CommandHookConfig {
    fn default() -> Self {
        Self { enabled: false, rules: DrcInput::default(), max_objects: 5000 }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HookDiagnostic {
    pub callback_id: String,
    pub phase: HookPhase,
    pub source_cursor: EventCursor,
    pub message: String,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct CommandHookStatus {
    pub config: CommandHookConfig,
    pub generation: u64,
    pub pre_callbacks: Vec<String>,
    pub post_callbacks: Vec<String>,
    pub diagnostics: Vec<HookDiagnostic>,
    pub supported_scope: &'static str,
}
#[derive(Default)]
struct State {
    config: CommandHookConfig,
    generation: u64,
    pre: Vec<(String, PreCommandHook)>,
    post: Vec<(String, PostCommandHook)>,
    diagnostics: VecDeque<HookDiagnostic>,
}
#[derive(Clone, Default)]
pub(crate) struct Hooks(Arc<Mutex<State>>);
impl std::fmt::Debug for Hooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hooks").finish_non_exhaustive()
    }
}
impl Hooks {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
    pub(crate) fn status(&self) -> CommandHookStatus {
        let s = self.state();
        CommandHookStatus {
            config: s.config.clone(),
            generation: s.generation,
            pre_callbacks: std::iter::once(BUILTIN.to_owned()).filter(|_| s.config.enabled).chain(s.pre.iter().map(|(id, _)| id.clone())).collect(),
            post_callbacks: s.post.iter().map(|(id, _)| id.clone()).collect(),
            diagnostics: s.diagnostics.iter().cloned().collect(),
            supported_scope: "Borrowed scoped PCB DRC; unsupported checks remain disclosed. Incomplete, excluded, truncated and violating candidates veto. Undo escapes veto. Trusted callbacks have no execution deadline.",
        }
    }
    pub(crate) fn configure(&self, config: CommandHookConfig) -> Result<CommandHookStatus, EngineError> {
        check_write()?;
        config.rules.validate().map_err(|e| invalid(e.to_string()))?;
        if !(1..=10000).contains(&config.max_objects) {
            return Err(invalid("max_objects must be 1..10000"));
        }
        {
            let mut s = self.state();
            if s.config != config {
                let next = next_generation(&s)?;
                s.config = config;
                s.generation = next;
            }
        }
        Ok(self.status())
    }
    fn check_registration(s: &State, id: &str, count: usize, max: usize) -> Result<u64, EngineError> {
        if id == BUILTIN || id.is_empty() || id.len() > 64 || id.trim() != id || !id.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(invalid("callback ID must be 1..64 trimmed ASCII visible bytes, excluding builtin_drc"));
        }
        if s.pre.iter().any(|(key, _)| key == id) || s.post.iter().any(|(key, _)| key == id) {
            return Err(invalid("callback ID already registered"));
        }
        if count >= max {
            return Err(invalid("callback capacity exceeded"));
        }
        next_generation(s)
    }
    pub(crate) fn register_pre(&self, id: String, hook: PreCommandHook) -> Result<(), EngineError> {
        check_write()?;
        let mut s = self.state();
        let next = Self::check_registration(&s, &id, s.pre.len(), 15)?;
        s.pre.push((id, hook));
        s.generation = next;
        Ok(())
    }
    pub(crate) fn register_post(&self, id: String, hook: PostCommandHook) -> Result<(), EngineError> {
        check_write()?;
        let mut s = self.state();
        let next = Self::check_registration(&s, &id, s.post.len(), 16)?;
        s.post.push((id, hook));
        s.generation = next;
        Ok(())
    }
    pub(crate) fn remove(&self, id: &str) -> Result<bool, EngineError> {
        check_write()?;
        let mut s = self.state();
        let present = s.pre.iter().any(|(key, _)| key == id) || s.post.iter().any(|(key, _)| key == id);
        if present {
            let next = next_generation(&s)?;
            s.pre.retain(|(key, _)| key != id);
            s.post.retain(|(key, _)| key != id);
            s.generation = next;
        }
        Ok(present)
    }
    fn diagnose(&self, id: &str, phase: HookPhase, source_cursor: EventCursor, message: &str) {
        let (message, truncated) = bounded(message);
        let mut s = self.state();
        if s.diagnostics.len() == 64 {
            s.diagnostics.pop_front();
        }
        s.diagnostics.push_back(HookDiagnostic { callback_id: id.into(), phase, source_cursor, message, truncated });
    }
    pub(crate) fn pre(&self, input: &PreCommand<'_>, pending: usize) -> Result<(), EngineError> {
        let (config, callbacks, observers) = {
            let s = self.state();
            (s.config.clone(), s.pre.clone(), !s.post.is_empty())
        };
        if observers
            && (pending.saturating_add(native_pending_count()) >= MAX_PENDING
                || input.label.len() > MAX_TEXT
                || matches!(input.origin, Origin::Agent{id} if id.len()>MAX_TEXT))
        {
            return Err(invalid("post observation queue/metadata budget exceeded; drain pending hooks or shorten transaction metadata"));
        }
        if config.enabled {
            let result = invoke(HookPhase::Pre, || {
                crate::auto_drc::snapshot_budget(input.project, config.max_objects)?;
                let report = viamind_drc::run_drc(input.project, &config.rules).map_err(|e| e.to_string())?;
                if report.truncated
                    || report.incomplete_reason.is_some()
                    || report.coverage.excluded_part_count != 0
                    || report.coverage.excluded_details_truncated
                    || !report.violations.is_empty()
                {
                    return Err(
                        "scoped DRC candidate has violations, exclusions or incomplete coverage; inspect/repair candidate or disable gate".into()
                    );
                }
                Ok(())
            });
            if let Err(message) = result {
                self.diagnose(BUILTIN, HookPhase::Pre, input.source_cursor, &message);
                return Err(invalid(format!("{BUILTIN} veto: {}", bounded(&message).0)));
            }
        }
        for (id, callback) in callbacks {
            if let Err(message) = invoke(HookPhase::Pre, || callback(input)) {
                self.diagnose(&id, HookPhase::Pre, input.source_cursor, &message);
                return Err(invalid(format!("{id} veto: {}", bounded(&message).0)));
            }
        }
        Ok(())
    }
    pub(crate) fn post(&self, input: &PreCommand<'_>, tx: TxId, committed_cursor: EventCursor, pending: usize) -> Option<PostCommandDispatch> {
        let s = self.state();
        if s.post.is_empty() {
            return None;
        }
        if pending.saturating_add(native_pending_count()) >= MAX_PENDING {
            drop(s);
            self.diagnose(
                "post_queue",
                HookPhase::Post,
                input.source_cursor,
                "undo committed; post notification dropped because standalone queue is full",
            );
            return None;
        }
        let (label, mut abbreviated) = bounded(input.label);
        let origin = match input.origin {
            Origin::User => Origin::User,
            Origin::Agent { id } => {
                let (id, short) = bounded(id);
                abbreviated |= short;
                Origin::Agent { id }
            }
        };
        Some(PostCommandDispatch {
            fact: PostCommand {
                project_id: input.project.id,
                source_cursor: input.source_cursor,
                committed_cursor,
                tx,
                origin,
                label,
                kind: input.kind,
                configuration_generation: s.generation,
                delta_counts: DeltaCounts::from_deltas(input.deltas),
                metadata_abbreviated: abbreviated,
            },
            callbacks: s.post.clone(),
            hooks: self.clone(),
        })
    }
}
fn next_generation(s: &State) -> Result<u64, EngineError> {
    s.generation.checked_add(1).ok_or_else(|| invalid("configuration generation exhausted"))
}
/// Owned bounded commit fact and its original observer set. Run outside engine/native locks.
pub struct PostCommandDispatch {
    pub fact: PostCommand,
    callbacks: Vec<(String, PostCommandHook)>,
    hooks: Hooks,
}
impl std::fmt::Debug for PostCommandDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostCommandDispatch").field("fact", &self.fact).finish_non_exhaustive()
    }
}
impl PostCommandDispatch {
    pub fn run(self) {
        for (id, callback) in &self.callbacks {
            if let Err(message) = invoke(HookPhase::Post, || callback(&self.fact)) {
                self.hooks.diagnose(id, HookPhase::Post, self.fact.source_cursor, &message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, EngineHandle};
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    fn add(engine: &mut Engine, name: &str) -> Result<crate::ExecResult, EngineError> {
        engine.execute("add_part", json!({"designator":name,"value":"","footprint":"F","x_mm":0,"y_mm":0}), Origin::User)
    }
    fn add_handle(engine: &EngineHandle, name: &str) -> Result<crate::ExecResult, EngineError> {
        engine.execute("add_part", json!({"designator":name,"value":"","footprint":"F","x_mm":0,"y_mm":0}), Origin::User)
    }
    #[test]
    fn veto_preserves_full_session_document_log_redo_and_next_id() {
        let mut e = Engine::new(Project::new("hooks"));
        assert_eq!(add(&mut e, "R1").unwrap().tx, TxId(1));
        e.undo(Origin::User).unwrap();
        let before = e.session_state();
        let status = e.document_status();
        let redo = e.redo_history().to_vec();
        let log = e.log().clone();
        e.register_pre_command_hook("deny".into(), Arc::new(|_| Err("denied".into()))).unwrap();
        assert!(add(&mut e, "R2").unwrap_err().to_string().contains("deny veto"));
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
        assert_eq!(e.redo_history(), redo);
        assert_eq!(e.log().clone(), log);
        assert!(e.redo(Origin::User).is_err());
        assert_eq!(e.session_state(), before);
        assert_eq!(e.redo_history(), redo);
        e.remove_command_hook("deny").unwrap();
        assert_eq!(add(&mut e, "R2").unwrap().tx, TxId(2));
    }
    #[test]
    fn final_candidate_batch_repair_undo_escape_and_noop_skip() {
        let mut e = Engine::new(Project::new("repair"));
        let first = add(&mut e, "R1").unwrap().created[0];
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = calls.clone();
        e.register_pre_command_hook(
            "nonempty".into(),
            Arc::new(move |c| {
                count.fetch_add(1, Ordering::SeqCst);
                if c.project.board.parts.is_empty() { Err("empty".into()) } else { Ok(()) }
            }),
        )
        .unwrap();
        e.transaction(Origin::User, "repair", |tx| {
            tx.execute("delete_part", json!({"id":first}))?;
            tx.execute("add_part", json!({"designator":"R2","value":"","footprint":"F","x_mm":0,"y_mm":0}))?;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        e.transaction(Origin::User, "noop", |_| Ok(())).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        e.undo(Origin::User).unwrap();
        e.undo(Origin::User).unwrap();
        assert!(e.project().board.parts.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn post_runs_outside_engine_and_native_locks_and_isolates_failure() {
        let e = EngineHandle::new(Project::new("post"));
        let seen = Arc::new(AtomicBool::new(false));
        let flag = seen.clone();
        let callback = e.clone();
        e.register_post_command_hook("first".into(), Arc::new(|_| panic!("observer failed"))).unwrap();
        e.register_post_command_hook(
            "second".into(),
            Arc::new(move |fact| {
                assert_eq!(callback.event_cursor(), fact.committed_cursor);
                assert_eq!(callback.read(|p| p.board.parts.len()), 1);
                assert!(add_handle(&callback, "forbidden").is_err());
                assert!(callback.watch_events(Default::default()).is_err());
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
        e.with_native_file_guard(|_| {
            add_handle(&e, "R1").unwrap();
            assert!(!seen.load(Ordering::SeqCst));
        });
        assert!(seen.load(Ordering::SeqCst));
        assert_eq!(e.command_hook_status().diagnostics.len(), 1);
        assert_eq!(e.read(|p| p.board.parts.len()), 1);
    }
    #[test]
    fn pre_captured_reads_panic_is_veto_but_mutations_return_error() {
        let e = EngineHandle::new(Project::new("pre"));
        let callback = e.clone();
        e.register_pre_command_hook(
            "captured".into(),
            Arc::new(move |_| {
                assert!(add_handle(&callback, "blocked").is_err());
                callback.event_cursor();
                Ok(())
            }),
        )
        .unwrap();
        let before = e.session_state();
        assert!(add_handle(&e, "R1").unwrap_err().to_string().contains("re-entry refused"));
        assert_eq!(e.session_state(), before);
        e.remove_command_hook("captured").unwrap();
        add_handle(&e, "R1").unwrap();
    }
    #[test]
    fn bounded_standalone_queue_refuses_apply_allows_undo_and_snapshots_callbacks() {
        let mut e = Engine::new(Project::new("queue"));
        let facts = Arc::new(Mutex::new(Vec::new()));
        let got = facts.clone();
        e.register_post_command_hook(
            "observe".into(),
            Arc::new(move |f| {
                got.lock().unwrap().push(f.clone());
                Ok(())
            }),
        )
        .unwrap();
        for i in 0..32 {
            add(&mut e, &format!("R{i}")).unwrap();
        }
        let before = e.session_state();
        assert!(add(&mut e, "overflow").is_err());
        assert_eq!(e.session_state(), before);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.project().board.parts.len(), 31);
        assert!(e.command_hook_status().diagnostics[0].message.contains("dropped"));
        e.remove_command_hook("observe").unwrap();
        for dispatch in e.take_pending_command_hooks() {
            dispatch.run();
        }
        let facts = facts.lock().unwrap();
        assert_eq!(facts.len(), 32);
        assert_eq!(facts[0].delta_counts.parts, 1);
        assert_eq!(facts[0].tx, TxId(1));
        assert_eq!(facts[31].tx, TxId(32));
    }
    #[test]
    fn drc_gate_uses_real_coverage_and_preserves_auto_drc_on_veto() {
        let e = EngineHandle::new(Project::new("drc"));
        e.configure_command_hooks(CommandHookConfig { enabled: true, ..Default::default() }).unwrap();
        let before = e.session_state();
        let auto = e.auto_drc_status();
        assert!(add_handle(&e, "legacy").unwrap_err().to_string().contains("builtin_drc veto"));
        assert_eq!(e.session_state(), before);
        assert_eq!(e.auto_drc_status(), auto);
        e.configure_command_hooks(Default::default()).unwrap();
        add_handle(&e, "legacy").unwrap();
        e.configure_command_hooks(CommandHookConfig { enabled: true, ..Default::default() }).unwrap();
        e.undo(Origin::User).unwrap();
        assert!(e.redo(Origin::User).is_err());
    }
    #[test]
    fn metadata_admission_and_unicode_diagnostics_are_bounded() {
        let mut e = Engine::new(Project::new("bounds"));
        e.register_post_command_hook("post".into(), Arc::new(|_| Ok(()))).unwrap();
        assert!(
            e.transaction(Origin::User, &"x".repeat(1025), |tx| {
                tx.execute("add_part", json!({"designator":"R1","value":"","footprint":"F","x_mm":0,"y_mm":0}))?;
                Ok(())
            })
            .is_err()
        );
        assert!(e.project().board.parts.is_empty());
        e.register_pre_command_hook("unicode".into(), Arc::new(|_| Err("🦀".repeat(1000)))).unwrap();
        assert!(add(&mut e, "R1").is_err());
        let d = e.command_hook_status().diagnostics.pop().unwrap();
        assert_eq!(d.message.len(), 1024);
        assert!(d.truncated);
    }
}
