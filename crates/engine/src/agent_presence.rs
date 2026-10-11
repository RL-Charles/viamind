//! Bounded attention overlays, independent of document and shared selection.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::PoisonError,
    time::{Duration, Instant},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project};
use viamind_geom::Nm;

use crate::{EngineHandle, EventCursor};

pub const AGENT_PRESENCE_CAPACITY: usize = 32;
pub const AGENT_PRESENCE_LEASE: Duration = Duration::from_secs(30);
const MAX_COORDINATE: i64 = 10_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresencePoint {
    #[schemars(range(min = -10000000000_i64, max = 10000000000_i64))]
    pub x: Nm,
    #[schemars(range(min = -10000000000_i64, max = 10000000000_i64))]
    pub y: Nm,
}

/// One connection attends to either PCB or one explicitly identified sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresenceSurface {
    #[default]
    Pcb,
    Schematic {
        sheet: ObjectId,
    },
}
impl PresenceSurface {
    fn is_pcb(&self) -> bool {
        matches!(self, Self::Pcb)
    }
    fn exists(self, project: &Project) -> bool {
        match self {
            Self::Pcb => true,
            Self::Schematic { sheet } => project.schematic.sheets.contains_key(&sheet),
        }
    }
    fn contains(self, project: &Project, id: &ObjectId) -> bool {
        match self {
            Self::Pcb => board_object(project, id),
            Self::Schematic { sheet } => project.schematic.sheets.get(&sheet).and_then(|sheet| sheet.content.as_ref()).is_some_and(|content| {
                content.symbols.contains_key(id)
                    || content.wires.contains_key(id)
                    || content.junctions.contains_key(id)
                    || content.labels.contains_key(id)
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetAgentPresence {
    pub expected_project: ObjectId,
    pub expected_stream: ObjectId,
    #[serde(default, skip_serializing_if = "PresenceSurface::is_pcb")]
    pub surface: PresenceSurface,
    pub cursor: PresencePoint,
    #[schemars(length(max = 64))]
    pub selection: Vec<ObjectId>,
    #[schemars(length(max = 80))]
    pub working_on: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearAgentPresence {
    pub expected_project: ObjectId,
    pub expected_stream: ObjectId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentPresence {
    pub connection: u64,
    pub display_name: String,
    #[serde(skip_serializing_if = "PresenceSurface::is_pcb")]
    pub surface: PresenceSurface,
    pub cursor: PresencePoint,
    pub selection: Vec<ObjectId>,
    pub working_on: String,
    pub palette_index: u8,
    pub remaining_lease_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentPresenceSnapshot {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub agents: Vec<AgentPresence>,
}

#[derive(Debug)]
struct Lease {
    presence: AgentPresence,
    expires: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct PresenceStore {
    stream: Option<ObjectId>,
    leases: BTreeMap<u64, Lease>,
}

impl PresenceStore {
    fn refresh(&mut self, project: &Project, stream: ObjectId, now: Instant) {
        if self.stream != Some(stream) {
            self.leases.clear();
            self.stream = Some(stream);
        }
        self.leases.retain(|_, lease| lease.expires > now && lease.presence.surface.exists(project));
    }
    fn snapshot(&self, project: &Project, cursor: EventCursor, now: Instant) -> AgentPresenceSnapshot {
        let agents = self
            .leases
            .values()
            .map(|lease| {
                let mut presence = lease.presence.clone();
                presence.selection.retain(|id| presence.surface.contains(project, id));
                presence.remaining_lease_ms = lease.expires.saturating_duration_since(now).as_millis().min(30_000) as u64;
                presence
            })
            .collect();
        AgentPresenceSnapshot { project_id: project.id, source_cursor: cursor, agents }
    }
}

fn board_object(project: &Project, id: &ObjectId) -> bool {
    project.board.parts.contains_key(id) || project.board.tracks.contains_key(id) || project.board.vias.contains_key(id)
}

fn display_name(name: &str) -> String {
    let mut text = String::new();
    for ch in name.trim().chars().filter(|ch| !ch.is_control()).take(48) {
        if text.len() + ch.len_utf8() > 192 {
            break;
        }
        text.push(ch);
    }
    if text.trim().is_empty() { "Agent".to_owned() } else { text }
}

impl EngineHandle {
    /// Read bounded live attention without cloning the project or renewing leases.
    pub fn query_agent_presence(&self) -> AgentPresenceSnapshot {
        self.presence_with_clock(Instant::now)
    }

    fn presence_with_clock(&self, clock: impl FnOnce() -> Instant) -> AgentPresenceSnapshot {
        let engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
        let mut store = self.agent_presence.lock().unwrap_or_else(PoisonError::into_inner);
        let now = clock();
        store.refresh(engine.project(), engine.event_cursor().stream_id, now);
        store.snapshot(engine.project(), engine.event_cursor(), now)
    }

    /// `connection` is supplied by the trusted transport, never by tool input.
    pub fn set_agent_presence(&self, connection: u64, name: &str, input: SetAgentPresence) -> Result<AgentPresenceSnapshot, String> {
        self.set_presence_with_clock(connection, name, input, Instant::now)
    }

    fn set_presence_with_clock(
        &self,
        connection: u64,
        name: &str,
        input: SetAgentPresence,
        clock: impl FnOnce() -> Instant,
    ) -> Result<AgentPresenceSnapshot, String> {
        if connection == 0 {
            return Err("presence requires a trusted connection".into());
        }
        if input.selection.len() > 64 {
            return Err("presence selection requires at most 64 board IDs".into());
        }
        if input.working_on.len() > 256
            || input.working_on.chars().count() > 80
            || input.working_on.trim() != input.working_on
            || input.working_on.chars().any(char::is_control)
        {
            return Err("working_on must be trimmed, control-free and within 80 characters/256 UTF-8 bytes".into());
        }
        if !(-MAX_COORDINATE..=MAX_COORDINATE).contains(&input.cursor.x.0) || !(-MAX_COORDINATE..=MAX_COORDINATE).contains(&input.cursor.y.0) {
            return Err("presence cursor requires integer Nm coordinates within +/-10000 mm".into());
        }
        let (snapshot, wakers) = {
            let mut engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
            if input.expected_project != engine.project().id || input.expected_stream != engine.event_cursor().stream_id {
                return Err("project or stream changed; refresh presence context".into());
            }
            if !input.surface.exists(engine.project()) {
                return Err("presence requires an existing schematic sheet".into());
            }
            let mut unique = BTreeSet::new();
            if input.selection.iter().any(|id| !unique.insert(*id) || !input.surface.contains(engine.project(), id)) {
                return Err("presence selection requires distinct existing objects in the chosen surface".into());
            }
            let mut store = self.agent_presence.lock().unwrap_or_else(PoisonError::into_inner);
            let now = clock();
            store.refresh(engine.project(), engine.event_cursor().stream_id, now);
            if !store.leases.contains_key(&connection) && store.leases.len() >= AGENT_PRESENCE_CAPACITY {
                return Err("presence has 32 active leases; wait for expiry or clear your own marker".into());
            }
            let expires = now.checked_add(AGENT_PRESENCE_LEASE).ok_or("presence lease clock overflow")?;
            store.leases.insert(
                connection,
                Lease {
                    presence: AgentPresence {
                        connection,
                        display_name: display_name(name),
                        surface: input.surface,
                        cursor: input.cursor,
                        selection: input.selection,
                        working_on: input.working_on,
                        palette_index: (connection % 8) as u8,
                        remaining_lease_ms: 30_000,
                    },
                    expires,
                },
            );
            (store.snapshot(engine.project(), engine.event_cursor(), now), engine.wake_transient_ui())
        };
        for wake in wakers {
            wake();
        }
        Ok(snapshot)
    }

    /// Clear only the trusted caller's marker, without changing document state.
    pub fn clear_agent_presence(&self, connection: u64, input: ClearAgentPresence) -> Result<AgentPresenceSnapshot, String> {
        if connection == 0 {
            return Err("presence requires a trusted connection".into());
        }
        let (snapshot, wakers) = {
            let mut engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
            if input.expected_project != engine.project().id || input.expected_stream != engine.event_cursor().stream_id {
                return Err("project or stream changed; refresh presence context".into());
            }
            let mut store = self.agent_presence.lock().unwrap_or_else(PoisonError::into_inner);
            let now = Instant::now();
            store.refresh(engine.project(), engine.event_cursor().stream_id, now);
            let removed = store.leases.remove(&connection).is_some();
            let snapshot = store.snapshot(engine.project(), engine.event_cursor(), now);
            (snapshot, if removed { engine.wake_transient_ui() } else { Vec::new() })
        };
        for wake in wakers {
            wake();
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Origin;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn input(engine: &EngineHandle) -> SetAgentPresence {
        SetAgentPresence {
            expected_project: engine.read(|p| p.id),
            expected_stream: engine.event_cursor().stream_id,
            surface: PresenceSurface::Pcb,
            cursor: PresencePoint { x: Nm(1), y: Nm(-3) },
            selection: vec![],
            working_on: "Checking route".into(),
        }
    }

    #[test]
    fn strict_surface_forms_preserve_legacy_pcb_serialization() {
        use serde_json::json;
        let engine = EngineHandle::new(Project::new("attention"));
        let old = serde_json::to_value(input(&engine)).unwrap();
        assert!(old.get("surface").is_none());
        assert_eq!(serde_json::from_value::<SetAgentPresence>(old.clone()).unwrap().surface, PresenceSurface::Pcb);
        for surface in [
            json!(null),
            json!({"kind":"other"}),
            json!({"kind":"pcb","sheet":ObjectId::new()}),
            json!({"kind":"schematic"}),
            json!({"kind":"schematic","sheet":ObjectId::new(),"owner":1}),
        ] {
            let mut v = old.clone();
            v["surface"] = surface;
            assert!(serde_json::from_value::<SetAgentPresence>(v.clone()).is_err(), "{v}");
        }
        let response = engine.set_agent_presence(1, "pcb", input(&engine)).unwrap();
        assert!(serde_json::to_value(response).unwrap()["agents"][0].get("surface").is_none());
    }

    #[test]
    fn explicit_sheet_identity_capacity_switch_deleted_objects_and_sheet_pruning() {
        use serde_json::json;
        let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
        let sheet = project.schematic.sheets.values().next().unwrap().clone();
        let content = sheet.content.as_ref().unwrap();
        let symbol = content.symbols.values().next().unwrap();
        let pin = symbol.pins[0].id;
        let selected =
            vec![symbol.id, *content.wires.keys().next().unwrap(), *content.junctions.keys().next().unwrap(), *content.labels.keys().next().unwrap()];
        let empty = viamind_doc::Sheet { id: ObjectId::new(), name: "Empty".into(), content: None };
        project.schematic.sheets.insert(empty.id, empty.clone());
        let engine = EngineHandle::new(project.clone());
        let before = engine.session_state();
        let now = Instant::now();
        for connection in 1..=32 {
            engine.set_presence_with_clock(connection, "pcb", input(&engine), || now).unwrap();
        }
        let mut attention = input(&engine);
        attention.surface = PresenceSurface::Schematic { sheet: sheet.id };
        attention.selection = selected.clone();
        engine.set_presence_with_clock(1, "schematic", attention.clone(), || now).unwrap();
        let expected = engine.presence_with_clock(|| now);
        assert_eq!(expected.agents.len(), 32);
        assert_eq!(expected.agents[0].selection, selected);
        for (surface, ids) in [
            (PresenceSurface::Schematic { sheet: empty.id }, selected.clone()),
            (attention.surface, vec![pin]),
            (PresenceSurface::Schematic { sheet: ObjectId::new() }, vec![]),
        ] {
            let mut invalid = attention.clone();
            invalid.surface = surface;
            invalid.selection = ids;
            assert!(engine.set_presence_with_clock(1, "changed", invalid, || now).is_err());
            assert_eq!(engine.presence_with_clock(|| now), expected);
        }
        assert_eq!(engine.session_state(), before);
        assert!(engine.set_presence_with_clock(33, "new", attention.clone(), || now).unwrap_err().contains("32 active"));
        // Existing participant can attend to an empty sheet at capacity.
        let mut empty_attention = attention.clone();
        empty_attention.surface = PresenceSurface::Schematic { sheet: empty.id };
        empty_attention.selection.clear();
        engine.set_presence_with_clock(2, "empty", empty_attention, || now).unwrap();
        engine.execute("edit_schematic_sheet",json!({"expected_project":project.id,"sheet_id":sheet.id,"expected":sheet,"name":sheet.name,"edits":[{"kind":"delete","id":symbol.id}]}),Origin::User).unwrap();
        assert!(!engine.query_agent_presence().agents[0].selection.contains(&symbol.id));
        engine.undo(Origin::User).unwrap();
        assert_eq!(engine.query_agent_presence().agents[0].selection, selected);
        engine.execute("delete_schematic_sheet", json!({"expected_project":project.id,"expected":empty}), Origin::User).unwrap();
        assert!(engine.query_agent_presence().agents.iter().all(|a| a.connection != 2));
        engine.undo(Origin::User).unwrap();
        assert!(engine.query_agent_presence().agents.iter().all(|a| a.connection != 2), "undo never recreates a pruned marker");
        assert_eq!(engine.read(Clone::clone), project);
        assert_eq!(engine.presence_with_clock(|| now + AGENT_PRESENCE_LEASE).agents.len(), 0);
    }

    #[test]
    fn exact_deadline_capacity_renewal_and_failed_update_preserve_prior_lease() {
        let engine = EngineHandle::new(Project::new("attention"));
        let before = engine.session_state();
        let status = engine.document_status();
        let now = Instant::now();
        for connection in 1..=32 {
            engine.set_presence_with_clock(connection, "same name", input(&engine), || now).unwrap();
        }
        assert_eq!(engine.presence_with_clock(|| now).agents.len(), 32);
        assert!(engine.set_presence_with_clock(33, "peer", input(&engine), || now).unwrap_err().contains("32 active"));
        let mut invalid = input(&engine);
        invalid.selection = vec![ObjectId::new()];
        assert!(engine.set_presence_with_clock(1, "changed", invalid, || now + Duration::from_secs(20)).is_err());
        assert_eq!(engine.presence_with_clock(|| now).agents[0].remaining_lease_ms, 30_000);
        assert_eq!(engine.presence_with_clock(|| now).agents[0].display_name, "same name");
        assert_eq!(engine.presence_with_clock(|| now + AGENT_PRESENCE_LEASE - Duration::from_nanos(1)).agents.len(), 32);
        engine.set_presence_with_clock(1, "renewed", input(&engine), || now + Duration::from_secs(1)).unwrap();
        let at_deadline = engine.presence_with_clock(|| now + AGENT_PRESENCE_LEASE);
        assert_eq!(at_deadline.agents.len(), 1);
        assert_eq!(at_deadline.agents[0].connection, 1);
        assert_eq!(at_deadline.agents[0].remaining_lease_ms, 1000);
        assert!(engine.presence_with_clock(|| now + Duration::from_secs(31)).agents.is_empty());
        engine.set_presence_with_clock(33, "admitted after expiry", input(&engine), || now + Duration::from_secs(31)).unwrap();
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
    }

    #[test]
    fn lease_clock_is_sampled_after_both_locks_for_query_and_admission() {
        let engine = EngineHandle::new(Project::new("attention"));
        let now = Instant::now();
        let check_locks = || {
            assert!(engine.engine.try_lock().is_err());
            assert!(engine.agent_presence.try_lock().is_err());
            now
        };
        engine.set_presence_with_clock(1, "agent", input(&engine), check_locks).unwrap();
        assert_eq!(engine.presence_with_clock(check_locks).agents[0].remaining_lease_ms, 30_000);
        assert!(
            engine
                .presence_with_clock(|| {
                    check_locks();
                    now + AGENT_PRESENCE_LEASE
                })
                .agents
                .is_empty()
        );
    }

    #[test]
    fn shared_clone_own_clear_lifecycle_bounds_and_wakers_leave_session_untouched() {
        let engine = EngineHandle::new(Project::new("attention"));
        let peer = engine.clone();
        let before = engine.session_state();
        let events = engine.subscribe();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let weak_engine = Arc::downgrade(&engine.engine);
        let weak_presence = Arc::downgrade(&engine.agent_presence);
        engine.on_change(move || {
            assert!(weak_engine.upgrade().unwrap().try_lock().is_ok());
            assert!(weak_presence.upgrade().unwrap().try_lock().is_ok());
            c.fetch_add(1, Ordering::SeqCst);
        });
        engine.set_agent_presence(1, "same\nname", input(&engine)).unwrap();
        peer.set_agent_presence(2, "same\nname", input(&peer)).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(engine.query_agent_presence().agents[0].display_name, "samename");
        let guard = ClearAgentPresence { expected_project: before.project.id, expected_stream: before.cursor.stream_id };
        engine.clear_agent_presence(1, guard).unwrap();
        assert_eq!(peer.query_agent_presence().agents.iter().map(|a| a.connection).collect::<Vec<_>>(), [2]);
        engine.clear_agent_presence(1, guard).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 3);
        let mut invalid = input(&engine);
        invalid.cursor.x = Nm(10_000_000_001);
        assert!(engine.set_agent_presence(2, "peer", invalid).is_err());
        let mut invalid = input(&engine);
        invalid.working_on = "🦀".repeat(65);
        assert!(engine.set_agent_presence(2, "peer", invalid).is_err());
        assert!(engine.set_agent_presence(0, "peer", input(&engine)).is_err());
        assert_eq!(engine.session_state(), before);
        assert!(events.try_recv().is_err());
        // Reopen of the same UUID still changes the engine stream.
        engine.engine.lock().unwrap().replace_document(before.project.clone(), None, Origin::User);
        assert_ne!(engine.event_cursor().stream_id, before.cursor.stream_id);
        assert!(engine.query_agent_presence().agents.is_empty());
        assert!(engine.clear_agent_presence(2, guard).is_err());
    }
}
