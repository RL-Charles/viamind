//! One guarded through-via transition between exact opposite-side explicit pads.
use crate::{Command, CommandError, Delta, EventCursor, PadEndpointId, RoutePolyline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project, Via};
use viamind_geom::{Nm, Point};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePadViaPolyline {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: PadEndpointId,
    pub target: PadEndpointId,
    pub via: Point,
    #[serde(default)]
    pub before: Vec<Point>,
    #[serde(default)]
    pub after: Vec<Point>,
    pub grid: Nm,
    pub width: Nm,
    pub via_diameter: Nm,
    pub via_drill: Nm,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for RoutePadViaPolyline {
    const NAME: &'static str = "route_pad_via_polyline";
    const DESCRIPTION: &'static str = "Atomically route between same-net explicit quarter-turn SMD pads on opposite outer copper sides through exactly one new plated through via. Resolve exact integer off-grid pad centers and derive net/layers from persisted pad IDs under exact expected_project/initial expected_cursor guards. via and before/after interior arrays must be grid aligned; their combined length is at most 1022. Width, via_diameter and via_drill are positive integer nm; drill < diameter. Shared joint bounded board-only scoped DRC checks new tracks and via, including drill/annular and physical hole spacing; incomplete/unsafe/stale proposals add no copper. One undo/redo preserves all IDs. No inner layers, blind/buried/microvias, multiple transitions, obstacle avoidance, shove or verified continuity/guide suppression.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected_project != project.id {
            return Err(CommandError::conflict("project changed; restart via route"));
        }
        if self.before.len().saturating_add(self.after.len()) > 1022 {
            return Err(CommandError::invalid("interior", "before/after combined must contain at most 1022 vertices"));
        }
        crate::route_endpoints::capture_budget(project)?;
        let (start, start_layer, start_net) = crate::route_endpoints::resolve(project, self.start)?;
        let (target, target_layer, target_net) = crate::route_endpoints::resolve(project, self.target)?;
        if self.start == self.target || start_layer == target_layer {
            return Err(CommandError::invalid("target", "one through-via route requires distinct pads on opposite outer copper sides"));
        }
        if start_net != target_net {
            return Err(CommandError::invalid("target", "pad endpoints must share the exact assigned net"));
        }
        if viamind_router::snap(self.via, self.grid).map_err(|e| CommandError::invalid("via", e))? != self.via {
            return Err(CommandError::invalid("via", "via position must lie exactly on the requested grid"));
        }
        let net = start_net.to_owned();
        let via = Via { id: ObjectId::new(), position: self.via, diameter: self.via_diameter, drill: self.via_drill, net: Some(net.clone()) };
        via.validate().map_err(|e| CommandError::invalid("via", e))?;
        validate_via_minima(project, &via, &self.rules)?;
        let mut first = Vec::with_capacity(self.before.len() + 2);
        first.push(start);
        first.extend(self.before);
        first.push(self.via);
        let mut second = Vec::with_capacity(self.after.len() + 2);
        second.push(self.via);
        second.extend(self.after);
        second.push(target);
        let leg = |points, layer| {
            RoutePolyline {
                expected_project: self.expected_project,
                expected_cursor: self.expected_cursor,
                points,
                grid: self.grid,
                width: self.width,
                layer,
                net: net.clone(),
                rules: self.rules.clone(),
            }
            .prepare_segments(project, true)
        };
        let mut tracks = leg(first, start_layer)?;
        tracks.extend(leg(second, target_layer)?);
        crate::routing::apply_route_candidate(project, tracks, vec![via], &self.rules)
    }
}

/// The same central document/request floors are rechecked by final joint DRC.
pub(crate) fn validate_via_minima(project: &Project, via: &Via, input: &viamind_drc::DrcInput) -> Result<(), CommandError> {
    viamind_doc::named_net_rule_budget(&project.board.net_rules).map_err(|e| CommandError::invalid("rules", e))?;
    let rules = input.validate().map_err(|e| CommandError::invalid("rules", e.to_string()))?;
    let drill = viamind_doc::net_rules::named_via_drill(&project.board.net_rules, rules.minimum_via_drill, via.net.as_deref());
    let annulus = viamind_doc::net_rules::named_annular_width(&project.board.net_rules, rules.minimum_annular_width, via.net.as_deref());
    let actual = via.diameter.0.checked_sub(via.drill.0).ok_or_else(|| CommandError::invalid("via", "annular dimension overflow"))?;
    let required = annulus.0.checked_mul(2).ok_or_else(|| CommandError::invalid("via", "annular minimum overflow"))?;
    if via.drill < drill || actual < required {
        return Err(CommandError::invalid(
            "via",
            format!("explicit via requires drill >= {} Nm and radial annular width >= {} Nm; no resizing", drill.0, annulus.0),
        ));
    }
    Ok(())
}
