//! Bounded transient command sandboxes. Only internally prepared deltas may merge.
mod budget;
#[cfg(test)]
mod tests;

use crate::{CommandRegistry, Delta, EngineHandle, EventCursor, ExecResult, Origin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use viamind_doc::{Board, ObjectId, Project};

const DATA_LIMIT: usize = 2 * 1024 * 1024;
const ROW_LIMIT: usize = 5000;
const DELTA_LIMIT: usize = 128;
const BRANCH_LIMIT: usize = 4;
const INPUT_LIMIT: usize = 64 * 1024;
const LEASE: Duration = Duration::from_secs(300);
static WORK_ACTIVE: AtomicBool = AtomicBool::new(false);
const COMMANDS: &[&str] = &[
    "add_part",
    "move_part",
    "delete_part",
    "update_part",
    "transform_parts",
    "set_part_pads",
    "add_track",
    "delete_track",
    "add_via",
    "delete_via",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("PCB branch: {message}")]
pub struct BranchError {
    pub message: String,
}
impl BranchError {
    fn invalid(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}
/// Trusted in-process actor. Never deserialize an actor from tool input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchActor {
    connection: Option<u64>,
    origin: Origin,
}
impl BranchActor {
    pub fn human() -> Self {
        Self { connection: None, origin: Origin::User }
    }
    pub fn agent(connection: u64, display: &str) -> Self {
        let display: String = display
            .chars()
            .filter(|c| !c.is_control())
            .scan(0usize, |bytes, c| {
                *bytes += c.len_utf8();
                (*bytes <= 128).then_some(c)
            })
            .collect();
        Self { connection: Some(connection), origin: Origin::agent(display) }
    }
    fn owns(&self, owner: &Self) -> bool {
        self.connection.is_none() || self.connection == owner.connection
    }
}
/// Move this token into the actual blocking worker, including canceled awaiters.
#[derive(Debug)]
pub struct BranchWorkPermit {
    _private: (),
}
impl Drop for BranchWorkPermit {
    fn drop(&mut self) {
        WORK_ACTIVE.store(false, Ordering::Release);
    }
}
pub fn reserve_pcb_branch_work() -> Result<BranchWorkPermit, BranchError> {
    WORK_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| BranchError::invalid("another PCB branch worker is active; retry after completion"))?;
    Ok(BranchWorkPermit { _private: () })
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePcbBranch {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub label: String,
}
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbBranchCall {
    pub name: String,
    pub input: serde_json::Value,
}
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyPcbBranch {
    pub branch_id: ObjectId,
    pub expected_revision: u64,
    pub commands: Vec<PcbBranchCall>,
}
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbBranchId {
    pub branch_id: ObjectId,
}
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbBranchRevision {
    pub branch_id: ObjectId,
    pub expected_revision: u64,
}
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MergePcbBranch {
    pub branch_id: ObjectId,
    pub expected_revision: u64,
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
}
fn default_limit() -> usize {
    32
}
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbBranchDiffInput {
    pub branch_id: ObjectId,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PcbBranchInfo {
    pub branch_id: ObjectId,
    pub label: String,
    pub creator: Origin,
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub revision: u64,
    pub changed_objects: usize,
    pub stale: bool,
    pub busy: bool,
    pub remaining_ms: u64,
}
#[derive(Debug, Serialize)]
pub struct PcbBranchState {
    pub branch: PcbBranchInfo,
    pub board: Board,
}
#[derive(Debug, Serialize)]
pub struct PcbBranchDiff {
    pub branch: PcbBranchInfo,
    pub offset: usize,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub deltas: Vec<Delta>,
}
#[derive(Debug, Serialize)]
pub struct PcbBranchApplied {
    pub branch: PcbBranchInfo,
    pub created: Vec<ObjectId>,
}

struct Branch {
    id: ObjectId,
    label: String,
    owner: BranchActor,
    project: Project,
    source: EventCursor,
    revision: u64,
    deltas: Vec<Delta>,
    deadline: Instant,
    busy: bool,
}
pub(crate) struct BranchStore {
    branches: BTreeMap<ObjectId, Branch>,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
}
impl Default for BranchStore {
    fn default() -> Self {
        Self { branches: BTreeMap::new(), clock: Arc::new(Instant::now) }
    }
}
impl std::fmt::Debug for BranchStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BranchStore").field("count", &self.branches.len()).finish()
    }
}
impl BranchStore {
    fn purge(&mut self, now: Instant) {
        self.branches.retain(|_, b| b.busy || b.deadline > now);
    }
    fn get(&self, id: ObjectId) -> Result<&Branch, BranchError> {
        self.branches.get(&id).ok_or_else(|| BranchError::invalid("unknown or expired branch; create a fresh branch"))
    }
}
impl Branch {
    fn info(&self, current: EventCursor, now: Instant) -> PcbBranchInfo {
        PcbBranchInfo {
            branch_id: self.id,
            label: self.label.clone(),
            creator: self.owner.origin.clone(),
            project_id: self.project.id,
            source_cursor: self.source,
            revision: self.revision,
            changed_objects: self.deltas.len(),
            stale: current != self.source,
            busy: self.busy,
            remaining_ms: self.deadline.saturating_duration_since(now).as_millis().try_into().unwrap_or(u64::MAX),
        }
    }
    fn owned(&self, actor: &BranchActor) -> Result<(), BranchError> {
        if actor.owns(&self.owner) { Ok(()) } else { Err(BranchError::invalid("this branch belongs to another transport connection")) }
    }
    fn ready(&self, revision: u64) -> Result<(), BranchError> {
        if self.busy {
            return Err(BranchError::invalid("branch worker is busy; wait for completion"));
        }
        if revision != self.revision {
            return Err(BranchError::invalid("branch revision changed; inspect it again"));
        }
        Ok(())
    }
}
fn label(label: &str) -> Result<(), BranchError> {
    if label.is_empty() || label.len() > 128 || label.trim().is_empty() || label.chars().any(char::is_control) {
        return Err(BranchError::invalid("label needs 1–128 UTF-8 bytes, nonblank and without controls"));
    }
    Ok(())
}
fn delta_key(delta: &Delta) -> (u8, ObjectId) {
    (
        match delta {
            Delta::Part { .. } => 0,
            Delta::Track { .. } => 1,
            Delta::Via { .. } => 2,
            _ => 3,
        },
        delta.id(),
    )
}
fn changes(mut deltas: Vec<Delta>) -> Result<Vec<Delta>, BranchError> {
    deltas = crate::coalesce(deltas);
    if deltas.len() > DELTA_LIMIT {
        return Err(BranchError::invalid("branch exceeds 128 changed objects"));
    }
    if deltas.iter().any(|d| !matches!(d, Delta::Part { .. } | Delta::Track { .. } | Delta::Via { .. })) {
        return Err(BranchError::invalid("sandbox command returned an unsupported object kind"));
    }
    budget::count(&deltas, DATA_LIMIT)?;
    deltas.sort_by_key(delta_key);
    Ok(deltas)
}
/// RAII keeps failed/panicking private preparation from leaving a busy branch.
struct Preparation {
    store: Arc<Mutex<BranchStore>>,
    id: ObjectId,
    revision: u64,
}
impl Drop for Preparation {
    fn drop(&mut self) {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(b) = store.branches.get_mut(&self.id)
            && b.revision == self.revision
        {
            b.busy = false;
        }
    }
}

impl EngineHandle {
    /// Check a transport's borrowed JSON before decoding owned command inputs.
    pub fn validate_pcb_branch_input(input: &serde_json::Value) -> Result<(), BranchError> {
        if input.get("commands").and_then(serde_json::Value::as_array).is_some_and(|v| v.len() > 64) {
            return Err(BranchError::invalid("branch applications support at most 64 commands"));
        }
        budget::count(input, INPUT_LIMIT)?;
        Ok(())
    }
    fn branch_wake(&self) {
        let wakers = self.engine.lock().unwrap_or_else(PoisonError::into_inner).wake_transient_ui();
        for waker in wakers {
            waker();
        }
    }
    pub fn create_pcb_branch(&self, input: CreatePcbBranch, actor: BranchActor, _permit: BranchWorkPermit) -> Result<PcbBranchInfo, BranchError> {
        label(&input.label)?;
        let result = self.with_engine(|engine| {
            if engine.project().id != input.expected_project || engine.event_cursor() != input.expected_cursor {
                return Err(BranchError::invalid("document changed; refresh session state before creating a branch"));
            }
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            if store.branches.len() >= BRANCH_LIMIT {
                return Err(BranchError::invalid("four PCB branches already exist; discard one or wait for expiry"));
            }
            budget::project(engine.project())?;
            let branch = Branch {
                id: ObjectId::new(),
                label: input.label,
                owner: actor,
                project: engine.project().clone(),
                source: input.expected_cursor,
                revision: 0,
                deltas: Vec::new(),
                deadline: now + LEASE,
                busy: false,
            };
            let info = branch.info(engine.event_cursor(), now);
            store.branches.insert(branch.id, branch);
            Ok(info)
        });
        if result.is_ok() {
            self.branch_wake();
        }
        result
    }
    pub fn list_pcb_branches(&self) -> Vec<PcbBranchInfo> {
        self.with_engine(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            store.branches.values().map(|b| b.info(engine.event_cursor(), now)).collect()
        })
    }
    pub fn inspect_pcb_branch(&self, input: PcbBranchId, actor: &BranchActor) -> Result<PcbBranchState, BranchError> {
        self.with_engine(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            let b = store.get(input.branch_id)?;
            b.owned(actor)?;
            Ok(PcbBranchState { branch: b.info(engine.event_cursor(), now), board: b.project.board.clone() })
        })
    }
    pub fn diff_pcb_branch(&self, input: PcbBranchDiffInput) -> Result<PcbBranchDiff, BranchError> {
        if !(1..=128).contains(&input.limit) {
            return Err(BranchError::invalid("diff limit must be within 1..128"));
        }
        self.with_engine(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            let b = store.get(input.branch_id)?;
            let end = input.offset.saturating_add(input.limit).min(b.deltas.len());
            let deltas = if input.offset >= b.deltas.len() { Vec::new() } else { b.deltas[input.offset..end].to_vec() };
            Ok(PcbBranchDiff {
                branch: b.info(engine.event_cursor(), now),
                offset: input.offset,
                total: b.deltas.len(),
                next_offset: (end < b.deltas.len()).then_some(end),
                deltas,
            })
        })
    }
    pub fn discard_pcb_branch(&self, input: PcbBranchRevision, actor: &BranchActor) -> Result<PcbBranchInfo, BranchError> {
        let result = self.with_engine(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            let b = store.get(input.branch_id)?;
            b.owned(actor)?;
            b.ready(input.expected_revision)?;
            let info = b.info(engine.event_cursor(), now);
            store.branches.remove(&input.branch_id);
            Ok(info)
        });
        if result.is_ok() {
            self.branch_wake();
        }
        result
    }
    pub fn apply_pcb_branch(&self, input: ApplyPcbBranch, actor: &BranchActor, permit: BranchWorkPermit) -> Result<PcbBranchApplied, BranchError> {
        self.apply_pcb_branch_with(input, actor, permit, || {})
    }
    fn apply_pcb_branch_with(
        &self,
        input: ApplyPcbBranch,
        actor: &BranchActor,
        _permit: BranchWorkPermit,
        prepared: impl FnOnce(),
    ) -> Result<PcbBranchApplied, BranchError> {
        if input.commands.is_empty() || input.commands.len() > 64 {
            return Err(BranchError::invalid("branch applications need 1..64 commands"));
        }
        budget::count(&input, INPUT_LIMIT)?;
        for call in &input.commands {
            if call.name.len() > 64 || !COMMANDS.contains(&call.name.as_str()) {
                return Err(BranchError::invalid(format!("unsupported sandbox command; supported: {}", COMMANDS.join(", "))));
            }
        }
        let (mut candidate, mut deltas, source) = self.with_engine(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            let b = store.get(input.branch_id)?;
            b.owned(actor)?;
            b.ready(input.expected_revision)?;
            if b.source.stream_id != engine.event_cursor().stream_id || b.project.id != engine.project().id {
                return Err(BranchError::invalid("branch belongs to a replaced document; inspect/discard and create a fresh branch"));
            }
            let result = (b.project.clone(), b.deltas.clone(), b.source);
            store.branches.get_mut(&input.branch_id).expect("checked branch").busy = true;
            Ok(result)
        })?;
        let _preparation = Preparation { store: self.pcb_branches.clone(), id: input.branch_id, revision: input.expected_revision };
        prepared();
        let registry = CommandRegistry::builtin();
        for (index, call) in input.commands.into_iter().enumerate() {
            let step = registry
                .dispatch_with_cursor(&call.name, call.input, &mut candidate, Some(source))
                .map_err(|e| BranchError::invalid(format!("sandbox step {index} failed; branch unchanged: {e}")))?;
            budget::project(&candidate)?;
            deltas.extend(step);
            deltas = changes(deltas)?;
        }
        let result = self.with_engine(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            let b = store
                .branches
                .get_mut(&input.branch_id)
                .ok_or_else(|| BranchError::invalid("branch disappeared during preparation; create a fresh branch"))?;
            if b.revision != input.expected_revision || !b.busy {
                return Err(BranchError::invalid("branch changed during preparation"));
            }
            if b.source.stream_id != engine.event_cursor().stream_id || b.project.id != engine.project().id {
                return Err(BranchError::invalid("document replaced during private preparation; branch unchanged"));
            }
            if b.project == candidate {
                return Err(BranchError::invalid("sandbox commands made no net change"));
            }
            let revision = b.revision.checked_add(1).ok_or_else(|| BranchError::invalid("branch revision exhausted; discard it"))?;
            b.project = candidate;
            b.deltas = deltas;
            b.revision = revision;
            b.deadline = now + LEASE;
            b.busy = false;
            let created = b.deltas.iter().filter(|d| d.is_create()).map(Delta::id).collect();
            Ok(PcbBranchApplied { branch: b.info(engine.event_cursor(), now), created })
        });
        if result.is_ok() {
            self.branch_wake();
        }
        result
    }
    pub fn merge_pcb_branch(&self, input: MergePcbBranch, actor: &BranchActor) -> Result<ExecResult, BranchError> {
        self.mutate(|engine| {
            let mut store = self.pcb_branches.lock().unwrap_or_else(PoisonError::into_inner);
            let now = (store.clock)();
            store.purge(now);
            let b = store.get(input.branch_id)?;
            b.owned(actor)?;
            b.ready(input.expected_revision)?;
            if input.expected_project != b.project.id || input.expected_cursor != b.source {
                return Err(BranchError::invalid("merge requires the original branch project/full cursor; inspect the branch"));
            }
            let result = engine.commit_prepared_pcb_branch(b.project.id, b.source, b.deltas.clone(), b.owner.origin.clone(), &b.label)?;
            store.branches.remove(&input.branch_id);
            Ok(result)
        })
    }
}
