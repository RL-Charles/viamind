//! Bounded static cardinal routing. Query and commit share geometry and complete joint preflight.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor, PadEndpointId, RoutePolyline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use viamind_doc::{CopperLayer, ObjectId, Project, Side};
use viamind_geom::{Nm, Point, Polygon, Rect};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct Permit;
impl Permit {
    pub(crate) fn acquire() -> Result<Self, CommandError> {
        let mut current = ACTIVE.load(Ordering::Acquire);
        loop {
            if current >= 2 {
                return Err(CommandError::conflict("walkaround busy; two planning/preflight operations already retained"));
            }
            match ACTIVE.compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(Self),
                Err(next) => current = next,
            }
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::Release);
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePadWalkaround {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: PadEndpointId,
    pub target: PadEndpointId,
    pub grid: Nm,
    pub width: Nm,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
#[derive(Clone, Debug, Serialize)]
pub struct CheckedWalkaround {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub layer: CopperLayer,
    pub net: String,
    pub plan: viamind_router::WalkaroundPlan,
    /// Complete for the inherited scoped explicit-copper/outline checks on this snapshot.
    pub complete_scoped_preflight: bool,
}
impl Command for RoutePadWalkaround {
    const NAME: &'static str = "route_pad_walkaround";
    const DESCRIPTION: &'static str = "Recompute and atomically commit bounded static cardinal walkaround between exact same-net/same-side quarter-turn explicit pad IDs. Exact expected_project/full initial expected_cursor required. Axis-aligned four-corner rectangular outer outline and rectangular cutouts only. Every substrate cutout is excluded by outward route half-width plus board-edge clearance plus1Nm, independently of copper/hole rules, within the shared64-obstacle budget. Conservative integer clearance boxes cover foreign pads/tracks/via copper and ALL via drill holes regardless of net. Legacy route-side parts, unsupported transforms, unsafe exact endpoint stubs, no path or budgets refuse without mutation. At most64 obstacle boxes,20000 nodes,100000 edges,4000000 intersection tests,510 interior waypoints,1024 segments; two retained planning/preflight operations. Complete shared scoped joint DRC required. Same-net copper unchanged; no shove, existing copper relocation,45-degree walkaround, global optimality or physical connectivity certification. One undo/redo preserves all IDs.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let _permit = Permit::acquire()?;
        let (_, deltas) = prepare(project, &self)?;
        for delta in &deltas {
            delta.apply_to(project);
        }
        Ok(deltas)
    }
}
impl EngineHandle {
    /// Capture only bounded board data under the engine lock. Search and preflight run outside it.
    pub fn plan_pad_walkaround(&self, input: &RoutePadWalkaround) -> Result<CheckedWalkaround, CommandError> {
        let _permit = Permit::acquire()?;
        let project = self.with_engine(|e| {
            if e.event_cursor() != input.expected_cursor || e.project().id != input.expected_project {
                return Err(CommandError::conflict("walkaround session changed; restart with the current project and full cursor"));
            }
            capture(e.project())?;
            Ok(Project { id: e.project().id, name: String::new(), schematic: Default::default(), board: e.project().board.clone() })
        })?;
        prepare(&project, input).map(|(plan, _)| plan)
    }
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("walkaround", message)
}
fn capture(project: &Project) -> Result<(), CommandError> {
    // Reserve the largest possible candidate before any board clone or search allocation.
    crate::auto_drc::snapshot_budget(project, 100_000 - 1024).map_err(invalid)
}
fn half(n: Nm) -> Result<i128, CommandError> {
    if n.0 <= 0 || n.0 > 10_000_000_000 {
        return Err(invalid("positive dimensions within10000mm required"));
    }
    Ok((i128::from(n.0) + 1) / 2)
}
fn point(x: i128, y: i128) -> Result<Point, CommandError> {
    Ok(Point::new(
        Nm(i64::try_from(x).map_err(|_| invalid("box coordinate overflow"))?),
        Nm(i64::try_from(y).map_err(|_| invalid("box coordinate overflow"))?),
    ))
}
fn expanded(center_box: Rect, x: i128, y: i128) -> Result<Rect, CommandError> {
    Ok(Rect {
        min: point(i128::from(center_box.min.x.0) - x, i128::from(center_box.min.y.0) - y)?,
        max: point(i128::from(center_box.max.x.0) + x, i128::from(center_box.max.y.0) + y)?,
    })
}
fn rectangle_bounds(ring: &Polygon, label: &str) -> Result<Rect, CommandError> {
    if ring.points.len() != 4 {
        return Err(invalid(format!("walkaround requires four-corner axis-aligned rectangular {label}")));
    }
    let bounds = Rect::from_points(ring.points.iter().copied()).expect("four corners");
    if ring.points.iter().any(|p| !bounds.corners().contains(p)) || ring.edges().any(|e| e.a.x != e.b.x && e.a.y != e.b.y) {
        return Err(invalid(format!("walkaround requires axis-aligned rectangular {label}")));
    }
    Ok(bounds)
}
fn domain(project: &Project, inset: i128) -> Result<Rect, CommandError> {
    let outline = project.board.outline.as_ref().ok_or_else(|| invalid("explicit rectangular outline required"))?;
    outline.validate().map_err(|e| invalid(e.to_string()))?;
    let bounds = rectangle_bounds(&outline.outer, "outer outline")?;
    // Check every cutout even when its inflated exclusion would lie outside the
    // inset route domain. Unsupported substrate geometry must never be ignored.
    for cutout in &outline.cutouts {
        rectangle_bounds(cutout, "cutout")?;
    }
    let min = point(i128::from(bounds.min.x.0) + inset, i128::from(bounds.min.y.0) + inset)?;
    let max = point(i128::from(bounds.max.x.0) - inset, i128::from(bounds.max.y.0) - inset)?;
    if min.x >= max.x || min.y >= max.y {
        return Err(invalid("route width and edge clearance collapse rectangular domain"));
    }
    Ok(Rect { min, max })
}
fn validate_id(ids: &mut std::collections::BTreeSet<ObjectId>, key: ObjectId, value: ObjectId) -> Result<(), CommandError> {
    if key != value || key.as_uuid().is_nil() || !ids.insert(key) {
        return Err(invalid("board identities must be nonnil, unique and match map keys"));
    }
    Ok(())
}
fn prepare(project: &Project, input: &RoutePadWalkaround) -> Result<(CheckedWalkaround, Vec<Delta>), CommandError> {
    if project.id != input.expected_project {
        return Err(CommandError::conflict("walkaround project changed"));
    }
    validate_scene(project)?;
    let (start, layer, net) = crate::route_endpoints::resolve(project, input.start)?;
    let (target, other_layer, other_net) = crate::route_endpoints::resolve(project, input.target)?;
    if input.start == input.target || start == target || layer != other_layer || net != other_net {
        return Err(invalid("distinct same-side explicit pad centers with exact same assigned net required"));
    }
    let (domain, obstacles) = scene(project, layer, net, input.width, &input.rules, viamind_router::walkaround::MAX_WALKAROUND_OBSTACLES)?;
    let plan = viamind_router::plan_walkaround(start, target, input.grid, domain, &obstacles).map_err(invalid)?;
    let tracks = RoutePolyline {
        expected_project: input.expected_project,
        expected_cursor: input.expected_cursor,
        points: plan.path.points.clone(),
        grid: input.grid,
        width: input.width,
        layer,
        net: net.to_owned(),
        rules: input.rules.clone(),
    }
    .prepare_segments(project, true)?;
    let deltas = crate::routing::prepare_route_candidate(project, tracks, Vec::new(), &input.rules)?;
    Ok((
        CheckedWalkaround {
            project_id: project.id,
            source_cursor: input.expected_cursor,
            layer,
            net: net.to_owned(),
            plan,
            complete_scoped_preflight: true,
        },
        deltas,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_refuses_third_retained_operation_and_releases_on_drop() {
        let first = Permit::acquire().unwrap();
        let second = Permit::acquire().unwrap();
        assert!(Permit::acquire().is_err());
        drop(first);
        let replacement = Permit::acquire().unwrap();
        assert!(Permit::acquire().is_err());
        drop(second);
        drop(replacement);
        assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
    }
}

pub(crate) fn validate_scene(project: &Project) -> Result<(), CommandError> {
    capture(project)?;
    let mut ids = std::collections::BTreeSet::new();
    validate_id(&mut ids, project.id, project.id)?;
    for (key, part) in &project.board.parts {
        validate_id(&mut ids, *key, part.id)?;
        if let Some(pads) = &part.pads {
            if pads.0.len() > 1024 {
                return Err(invalid("at most1024 explicit pads per part"));
            }
            for pad in &pads.0 {
                validate_id(&mut ids, pad.id, pad.id)?;
            }
        }
    }
    for (key, t) in &project.board.tracks {
        validate_id(&mut ids, *key, t.id)?;
    }
    for (key, v) in &project.board.vias {
        validate_id(&mut ids, *key, v.id)?;
    }
    Ok(())
}

pub(crate) fn scene(
    project: &Project,
    layer: CopperLayer,
    net: &str,
    width: Nm,
    input: &viamind_drc::DrcInput,
    remaining_obstacles: usize,
) -> Result<(Rect, Vec<Rect>), CommandError> {
    let rules = input.validate().map_err(|e| invalid(e.to_string()))?;
    let r = half(width)?;
    let edge = r + i128::from(rules.edge_clearance.0) + 1;
    let domain = domain(project, edge)?;
    let copper = r + i128::from(rules.copper_clearance.0) + 1;
    let hole = r + i128::from(rules.hole_to_copper_clearance.0) + 1;
    let mut obstacles = Vec::new();
    let mut push = |bounds: Rect| -> Result<(), CommandError> {
        if let Some(clipped) = bounds.intersection(&domain) {
            if obstacles.len() >= remaining_obstacles {
                return Err(invalid("walkaround relevant obstacle box limit64 exceeded"));
            }
            obstacles.push(clipped);
        }
        Ok(())
    };
    // Substrate holes use board-edge clearance for every net/layer. They share
    // the same relevant-obstacle cap with copper and all via drill exclusions.
    for cutout in &project.board.outline.as_ref().expect("domain validated outline").cutouts {
        push(expanded(rectangle_bounds(cutout, "cutout")?, edge, edge)?)?;
    }
    for part in project.board.parts.values() {
        let side = if part.side == Side::Top { CopperLayer::Top } else { CopperLayer::Bottom };
        if side != layer {
            continue;
        }
        if !(0..360_000).contains(&part.rotation_mdeg) || [part.position.x.0, part.position.y.0].iter().any(|n| n.unsigned_abs() > 10_000_000_000) {
            return Err(invalid("route-side part requires normalized rotation and origin within±10000mm"));
        }
        let pads = part.pads.as_ref().ok_or_else(|| invalid("route-side legacy part lacks explicit pads; walkaround coverage refused"))?;
        viamind_doc::Pads::try_from(pads.0.clone()).map_err(invalid)?;
        for pad in &pads.0 {
            let center = part.local_to_world_nm_exact(pad.position).map_err(invalid)?;
            let (mut x, mut y) = (half(pad.width)?, half(pad.height)?);
            if part.rotation_mdeg.rem_euclid(180_000) == 90_000 {
                std::mem::swap(&mut x, &mut y);
            }
            if center.x.0.unsigned_abs() > 10_000_000_000 || center.y.0.unsigned_abs() > 10_000_000_000 {
                return Err(invalid("pad center out of range"));
            }
            if pad.net.as_deref() != Some(net) {
                push(expanded(Rect::from_point(center), x + copper, y + copper)?)?;
            }
        }
    }
    for track in project.board.tracks.values() {
        track.validate().map_err(invalid)?;
        if track.layer == layer && track.net.as_deref() != Some(net) {
            let margin = half(track.width)? + copper;
            push(expanded(Rect::new(track.start, track.end), margin, margin)?)?;
        }
    }
    for via in project.board.vias.values() {
        via.validate().map_err(invalid)?;
        if via.net.as_deref() != Some(net) {
            let margin = half(via.diameter)? + copper;
            push(expanded(Rect::from_point(via.position), margin, margin)?)?;
        }
        let margin = half(via.drill)? + hole;
        push(expanded(Rect::from_point(via.position), margin, margin)?)?;
    }
    Ok((domain, obstacles))
}
