//! Guarded straight polyline foundation. Validate the entire candidate before any mutation.
use crate::{Command, CommandError, Delta, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use viamind_doc::{CopperLayer, MAX_COPPER_OBJECTS, ObjectId, Project, Track};
use viamind_geom::{Nm, Point};

/// One user-directed route, atomically committed and undone through the shared engine.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePolyline {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    /// Two to 1025 integer-nanometre points, each exactly on the requested grid.
    pub points: Vec<Point>,
    pub grid: Nm,
    pub width: Nm,
    pub layer: CopperLayer,
    /// Exact trimmed nonempty net label. No automatic pad/net inference is performed.
    pub net: String,
    /// Existing scoped explicit-copper/outline rules; incomplete reports reject the route.
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for RoutePolyline {
    const NAME: &'static str = "route_polyline";
    const DESCRIPTION: &'static str = "Atomically add a snapped straight PCB polyline (2..1025 integer-nanometre points, positive grid/width) on top/bottom copper with an exact nonempty net. Requires project identity and exact expected_cursor from get_session_state; any stale stream/sequence rejects all segments. Explicit outline required. Bounded DRC rejects violations involving proposed tracks and incomplete reports, checking explicit pads/tracks/through vias and substrate/cutouts under supplied rules. Legacy parts without explicit pads and other run_drc exclusions remain unchecked; existing unrelated violations may remain. No push/shove, obstacle avoidance, pad attachment, via routing or electrical continuity verification. One undo/redo transaction.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected_project != project.id {
            return Err(CommandError::conflict("project changed; restart the route"));
        }
        if !(2..=1025).contains(&self.points.len()) {
            return Err(CommandError::invalid("points", "must contain two to 1025 vertices"));
        }
        if self.net.len() > 256
            || self.net.is_empty()
            || self.net.trim() != self.net
            || self.net.chars().count() > 128
            || self.net.chars().any(char::is_control)
        {
            return Err(CommandError::invalid("net", "must be trimmed, nonempty, control-free and within 128 characters/256 bytes"));
        }
        if self.grid.0 <= 0 || self.grid.0 > 10_000_000_000 {
            return Err(CommandError::invalid("grid", "must be positive and at most 10000 mm"));
        }
        if project.board.tracks.len().saturating_add(self.points.len() - 1) > MAX_COPPER_OBJECTS {
            return Err(CommandError::invalid("points", "route exceeds the 100000-track capacity"));
        }
        if project.board.outline.is_none() {
            return Err(CommandError::invalid("outline", "set an explicit board outline before routing"));
        }
        for point in &self.points {
            let snapped = viamind_router::snap(*point, self.grid).map_err(|e| CommandError::invalid("points", e))?;
            if snapped != *point {
                return Err(CommandError::invalid("points", "each vertex must lie exactly on the requested grid"));
            }
        }
        self.rules.validate().map_err(|e| CommandError::invalid("rules", e.to_string()))?;
        crate::auto_drc::snapshot_budget(project, 100_000 - (self.points.len() - 1)).map_err(|e| {
            CommandError::conflict(format!("route preflight cannot capture bounded board: {}", e.trim_start_matches("Automatic DRC skipped: ")))
        })?;
        // Capture only the bounded board; unrelated schematic content and project text are
        // neither inspected nor cloned while routing holds the engine lock.
        let mut candidate = Project { id: project.id, name: String::new(), schematic: Default::default(), board: project.board.clone() };
        let mut deltas = Vec::with_capacity(self.points.len() - 1);
        let mut ids = BTreeSet::new();
        for pair in self.points.windows(2) {
            let track =
                Track { id: ObjectId::new(), start: pair[0], end: pair[1], width: self.width, layer: self.layer, net: Some(self.net.clone()) };
            track.validate().map_err(|e| CommandError::invalid("route", e))?;
            ids.insert(track.id);
            let delta = Delta::Track { id: track.id, before: None, after: Some(track) };
            delta.apply_to(&mut candidate);
            deltas.push(delta);
        }
        crate::auto_drc::snapshot_budget(&candidate, 100_000).map_err(|e| {
            CommandError::conflict(format!("route preflight cannot capture bounded board: {}", e.trim_start_matches("Automatic DRC skipped: ")))
        })?;
        let report = viamind_drc::run_drc(&candidate, &self.rules).map_err(|e| CommandError::invalid("rules", e.to_string()))?;
        if report.truncated || report.incomplete_reason.is_some() {
            return Err(CommandError::conflict(format!(
                "route preflight incomplete; no segments added: {}",
                report.incomplete_reason.as_deref().unwrap_or("DRC result truncated")
            )));
        }
        if let Some(violation) = report.violations.iter().find(|v| v.objects.iter().any(|o| ids.contains(&o.selection_id()))) {
            return Err(CommandError::conflict(format!("route preflight failed; no segments added: {}", violation.message)));
        }
        for delta in &deltas {
            delta.apply_to(project);
        }
        Ok(deltas)
    }
}
