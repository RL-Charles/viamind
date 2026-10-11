//! Exact integer corner-mode planning for guarded same-side explicit pad routes.
use crate::{Command, CommandError, Delta, EventCursor, PadEndpointId, RoutePolyline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project};
use viamind_geom::{Nm, Point};
use viamind_router::{CornerMode, Posture};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePadPosture {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: PadEndpointId,
    pub target: PadEndpointId,
    #[serde(default)]
    pub waypoints: Vec<Point>,
    pub grid: Nm,
    pub width: Nm,
    pub mode: CornerMode,
    #[serde(default)]
    pub posture: Posture,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for RoutePadPosture {
    const NAME: &'static str = "route_pad_posture";
    const DESCRIPTION: &'static str = "Atomically plan and route same-side exact same-net explicit quarter-turn pad IDs using cardinal or octilinear (horizontal/vertical/45-degree) grid geometry. posture primary means horizontal-first/cardinal or straight-first/octilinear; alternate means vertical-first or diagonal-first. Up to510 supplied grid-aligned waypoints, positive grid/width integer Nm, at most1024 generated segments. Resolve exact off-grid centers with short explicit nearest-grid endpoint stubs; stubs may have arbitrary angles and are included in scoped DRC. Derive net/layer, require expected project/full initial cursor, bounded board-only capture, explicit outline and complete joint preflight. Unsafe/stale/malformed proposals add no copper. One undo/redo preserves IDs. No obstacle avoidance, walkaround/shove, via transition, rounded corners, pad-edge inference or full electrical continuity certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project {
            return Err(CommandError::conflict("project changed; restart posture route"));
        }
        if self.waypoints.len() > 510 {
            return Err(CommandError::invalid("waypoints", "at most 510 confirmed waypoints are supported"));
        }
        crate::route_endpoints::capture_budget(project)?;
        let (start, layer, net) = crate::route_endpoints::resolve(project, self.start)?;
        let (target, target_layer, target_net) = crate::route_endpoints::resolve(project, self.target)?;
        if self.start == self.target || start == target || layer != target_layer || net != target_net {
            return Err(CommandError::invalid("target", "choose distinct pad centers on the same copper side with the exact same assigned net"));
        }
        let plan = viamind_router::plan_pad_path(start, target, &self.waypoints, self.grid, self.mode, self.posture)
            .map_err(|error| CommandError::invalid("path", error))?;
        RoutePolyline {
            expected_project: self.expected_project,
            expected_cursor: self.expected_cursor,
            points: plan.points,
            grid: self.grid,
            width: self.width,
            layer,
            net: net.to_owned(),
            rules: self.rules,
        }
        .apply_resolved(project, true)
    }
}
