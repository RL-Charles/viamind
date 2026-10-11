//! Two static outer-side paths through one fixed via, checked as one candidate.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor, PadEndpointId, RoutePolyline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{CopperLayer, ObjectId, Project, Via};
use viamind_geom::{Nm, Point};
use viamind_router::{WalkaroundBudget, WalkaroundPlan, WalkaroundWork};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePadViaWalkaround {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: PadEndpointId,
    pub target: PadEndpointId,
    pub via: Point,
    pub grid: Nm,
    pub width: Nm,
    pub via_diameter: Nm,
    pub via_drill: Nm,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckedViaWalkaround {
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub document_net_rules: viamind_doc::NamedNetRules,
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub start_layer: CopperLayer,
    pub target_layer: CopperLayer,
    pub net: String,
    pub via: Point,
    pub via_diameter: Nm,
    pub via_drill: Nm,
    pub before: WalkaroundPlan,
    pub after: WalkaroundPlan,
    pub work: WalkaroundWork,
    pub complete_scoped_preflight: bool,
}

impl Command for RoutePadViaWalkaround {
    const NAME: &'static str = "route_pad_via_walkaround";
    const DESCRIPTION: &'static str = "Recompute and atomically commit two static cardinal outer-side walkaround legs through exactly one fixed new through via. Exact expected_project/full initial expected_cursor and opposite-side same-net explicit quarter-turn pad IDs required. Via center must be exactly grid aligned; no automatic via relocation. Rectangular outer outline and rectangular cutouts only, with existing conservative foreign copper and ALL drill hole exclusions. One captured board and shared admission cover both legs; aggregate64 boxes/20000 nodes/100000 edges/4000000 intersections/510 interior coordinates/1024 new tracks plus one via. Both legs and via require complete joint scoped DRC before any success; stale, incomplete, no-path, unsafe or budget requests leave design unchanged. One undo/redo preserves all IDs. No shove, general outlines, automatic via placement or physical certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let _permit = crate::walkaround::Permit::acquire()?;
        let (_, deltas) = prepare(project, &self)?;
        for delta in &deltas {
            delta.apply_to(project);
        }
        Ok(deltas)
    }
}
impl EngineHandle {
    pub fn plan_pad_via_walkaround(&self, input: &RoutePadViaWalkaround) -> Result<CheckedViaWalkaround, CommandError> {
        let _permit = crate::walkaround::Permit::acquire()?;
        let project = self.with_engine(|e| {
            if e.event_cursor() != input.expected_cursor || e.project().id != input.expected_project {
                return Err(CommandError::conflict("via walkaround session changed; refresh project and full cursor"));
            }
            capture(e.project())
        })?;
        prepare(&project, input).map(|(plan, _)| plan)
    }
}
fn capture(project: &Project) -> Result<Project, CommandError> {
    budget(project)?;
    Ok(Project { id: project.id, name: String::new(), schematic: Default::default(), board: project.board.clone_without_assembly() })
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("via_walkaround", message)
}

fn budget(project: &Project) -> Result<(), CommandError> {
    crate::auto_drc::snapshot_budget(project, 100_000 - 1025).map_err(invalid)
}
fn prepare(project: &Project, input: &RoutePadViaWalkaround) -> Result<(CheckedViaWalkaround, Vec<Delta>), CommandError> {
    if project.id != input.expected_project {
        return Err(CommandError::conflict("via walkaround project changed"));
    }
    budget(project)?;
    crate::walkaround::validate_scene(project)?;
    let (start, start_layer, net) = crate::route_endpoints::resolve(project, input.start)?;
    let (target, target_layer, other_net) = crate::route_endpoints::resolve(project, input.target)?;
    if input.start == input.target || start_layer == target_layer || net != other_net || start == input.via || target == input.via {
        return Err(invalid("distinct opposite-side same-net explicit pad centers and nonzero fixed-via legs required"));
    }
    if viamind_router::snap(input.via, input.grid).map_err(invalid)? != input.via {
        return Err(invalid("fixed via center must lie exactly on the requested grid"));
    }
    let via = Via { id: ObjectId::new(), position: input.via, diameter: input.via_diameter, drill: input.via_drill, net: Some(net.to_owned()) };
    via.validate().map_err(invalid)?;
    crate::route_via::validate_via_minima(project, &via, &input.rules)?;
    let mut work = WalkaroundBudget::default();
    let mut leg = |first, last, layer| {
        let remaining = viamind_router::walkaround::MAX_WALKAROUND_OBSTACLES - work.used().obstacle_boxes;
        let (domain, obstacles) = crate::walkaround::scene(project, layer, net, input.width, &input.rules, remaining)?;
        viamind_router::plan_walkaround_with_budget(first, last, input.grid, domain, &obstacles, &mut work).map_err(invalid)
    };
    let before = leg(start, input.via, start_layer)?;
    let after = leg(input.via, target, target_layer)?;
    // Count the emitted interior coordinates, including exact endpoint anchors.
    if before.path.points.len().saturating_sub(2).saturating_add(after.path.points.len().saturating_sub(2)) > 510 {
        return Err(invalid("combined via walkaround exceeds510 interior coordinates"));
    }
    let tracks = |points, layer| {
        RoutePolyline {
            expected_project: input.expected_project,
            expected_cursor: input.expected_cursor,
            points,
            grid: input.grid,
            width: input.width,
            layer,
            net: net.to_owned(),
            rules: input.rules.clone(),
        }
        .prepare_segments(project, true)
    };
    let mut candidate = tracks(before.path.points.clone(), start_layer)?;
    candidate.extend(tracks(after.path.points.clone(), target_layer)?);
    if candidate.len() > 1024 {
        return Err(invalid("combined via walkaround exceeds1024 new tracks"));
    }
    // Never expose a successful first leg or checked plan before joint DRC.
    let deltas = crate::routing::prepare_route_candidate(project, candidate, vec![via], &input.rules)?;
    Ok((
        CheckedViaWalkaround {
            document_net_rules: project.board.net_rules.clone(),
            project_id: project.id,
            source_cursor: input.expected_cursor,
            start_layer,
            target_layer,
            net: net.to_owned(),
            via: input.via,
            via_diameter: input.via_diameter,
            via_drill: input.via_drill,
            before,
            after,
            work: work.used().clone(),
            complete_scoped_preflight: true,
        },
        deltas,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_geometry_capture_omits_unused_invalid_sourcing() {
        let mut project = Project::new("Not needed by routing");
        let part = viamind_doc::Part {
            id: ObjectId::new(),
            schematic_symbol: None,
            schematic_package: None,
            designator: "J1".into(),
            value: String::new(),
            footprint: String::new(),
            position: Point::new(Nm(1), Nm(2)),
            rotation_mdeg: 90000,
            side: viamind_doc::Side::Bottom,
            pads: None,
            assembly: Some(Box::new(viamind_doc::AssemblyMetadata { mpn: Some("\n".repeat(3 * 1024 * 1024)), ..Default::default() })),
        };
        let id = part.id;
        project.board.parts.insert(id, part);
        assert!(viamind_doc::file::validate_assembly(&project).is_err());
        let snapshot = capture(&project).unwrap();
        assert!(snapshot.board.parts[&id].assembly.is_none());
        assert_eq!(snapshot.board.parts[&id], project.board.parts[&id].clone_without_assembly());
        assert!(snapshot.name.is_empty());
        assert_eq!(project.board.parts[&id].assembly.as_ref().unwrap().mpn.as_ref().unwrap().len(), 3 * 1024 * 1024);
    }
}
