//! Bounded explicit pad discovery and exact identity-resolved route endpoints.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor, RoutePolyline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{CopperLayer, ObjectId, Pad, Part, Project, Side};
use viamind_geom::{Nm, Point};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadEndpointId {
    pub part_id: ObjectId,
    pub pad_id: ObjectId,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteEndpoint {
    pub id: PadEndpointId,
    pub designator: String,
    pub number: String,
    pub net: Option<String>,
    pub layer: CopperLayer,
    pub center_nm: Option<Point>,
    /// Rendering/picking boundary only. These coordinates never become command endpoints.
    pub corners_mm: [[f64; 2]; 4],
    pub unsupported_reason: Option<String>,
}
impl RouteEndpoint {
    pub fn supported(&self) -> bool {
        self.unsupported_reason.is_none()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteEndpointSnapshot {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub endpoints: Vec<RouteEndpoint>,
    pub omitted_legacy_parts: usize,
    pub unsupported_endpoints: usize,
    /// Conservative remaining guide identities after bounded exact-endpoint copper proofs.
    pub guide_endpoints: Vec<[PadEndpointId; 2]>,
}
fn limit() -> usize {
    100
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouteEndpointQuery {
    pub net: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "limit")]
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteEndpointPage {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub endpoints: Vec<RouteEndpoint>,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub omitted_legacy_parts: usize,
    pub unsupported_endpoints: usize,
}
fn layer(part: &Part) -> CopperLayer {
    match part.side {
        Side::Top => CopperLayer::Top,
        Side::Bottom => CopperLayer::Bottom,
    }
}
pub(crate) fn capture_budget(project: &Project) -> Result<(), CommandError> {
    crate::auto_drc::snapshot_budget(project, 100_000).map_err(|e| {
        CommandError::conflict(format!("route endpoint discovery cannot capture bounded board: {}", e.trim_start_matches("Automatic DRC skipped: ")))
    })
}
pub(crate) fn resolve(project: &Project, id: PadEndpointId) -> Result<(Point, CopperLayer, &str), CommandError> {
    let part = project.board.part(&id.part_id).ok_or_else(|| CommandError::conflict("route endpoint part no longer exists"))?;
    let pad = part
        .pads
        .as_ref()
        .and_then(|p| p.0.iter().find(|p| p.id == id.pad_id))
        .ok_or_else(|| CommandError::conflict("route endpoint explicit pad no longer exists"))?;
    resolve_pad(part, pad)
}
pub(crate) fn resolve_pad<'a>(part: &Part, pad: &'a Pad) -> Result<(Point, CopperLayer, &'a str), CommandError> {
    let net = pad.net.as_deref().filter(|n| !n.is_empty()).ok_or_else(|| CommandError::invalid("endpoint", "pad must have an assigned net"))?;
    if net.len() > 256 {
        return Err(CommandError::invalid("endpoint", "pad net exceeds 256 bytes"));
    }
    let point = part.local_to_world_nm_exact(pad.position).map_err(|e| CommandError::invalid("endpoint", e))?;
    if point.x.0.unsigned_abs() > 10_000_000_000 || point.y.0.unsigned_abs() > 10_000_000_000 {
        return Err(CommandError::invalid("endpoint", "transformed pad center exceeds copper coordinate range"));
    }
    Ok((point, layer(part), net))
}
impl EngineHandle {
    pub fn route_endpoint_snapshot(&self) -> Result<RouteEndpointSnapshot, CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        self.with_engine(|engine| {
            let project = engine.project();
            capture_budget(project)?;
            let mut endpoints = Vec::new();
            let mut omitted = 0;
            for part in project.board.parts.values() {
                let Some(pads) = &part.pads else {
                    omitted += 1;
                    continue;
                };
                for pad in &pads.0 {
                    let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
                    let resolved = resolve_pad(part, pad);
                    let hw = pad.width.to_mm() / 2.0;
                    let hh = pad.height.to_mm() / 2.0;
                    let cx = pad.position.x.to_mm();
                    let cy = pad.position.y.to_mm();
                    let corners_mm = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| {
                        let (x, y) = part.local_to_world_mm(cx + x, cy + y);
                        [x, y]
                    });
                    endpoints.push(RouteEndpoint {
                        id,
                        designator: part.designator.clone(),
                        number: pad.number.clone(),
                        net: pad.net.clone(),
                        layer: layer(part),
                        center_nm: resolved.as_ref().ok().map(|r| r.0),
                        corners_mm,
                        unsupported_reason: resolved.err().map(|e| e.to_string()),
                    });
                }
            }
            endpoints.sort_by_key(|p| p.id);
            let unsupported_endpoints = endpoints.iter().filter(|p| !p.supported()).count();
            Ok(RouteEndpointSnapshot {
                project_id: project.id,
                source_cursor: engine.event_cursor(),
                endpoints,
                omitted_legacy_parts: omitted,
                unsupported_endpoints,
                guide_endpoints: crate::connectivity::connection_guide_ids(project),
            })
        })
    }
    pub fn query_route_endpoints(&self, query: &RouteEndpointQuery) -> Result<RouteEndpointPage, CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        if !(1..=1000).contains(&query.limit) {
            return Err(CommandError::invalid("limit", "must be within 1..1000"));
        }
        if query.net.as_ref().is_some_and(|s| s.len() > 256) {
            return Err(CommandError::invalid("net", "must be within 256 bytes"));
        }
        let snapshot = self.route_endpoint_snapshot()?;
        let matching: Vec<_> =
            snapshot.endpoints.into_iter().filter(|p| query.net.as_deref().is_none_or(|net| p.net.as_deref() == Some(net))).collect();
        let total = matching.len();
        let unsupported_endpoints = matching.iter().filter(|p| !p.supported()).count();
        let end = query.offset.saturating_add(query.limit);
        Ok(RouteEndpointPage {
            project_id: snapshot.project_id,
            source_cursor: snapshot.source_cursor,
            endpoints: matching.into_iter().skip(query.offset).take(query.limit).collect(),
            total,
            next_offset: (end < total).then_some(end),
            omitted_legacy_parts: snapshot.omitted_legacy_parts,
            unsupported_endpoints,
        })
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePadPolyline {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: PadEndpointId,
    pub target: PadEndpointId,
    #[serde(default)]
    pub interior: Vec<Point>,
    pub grid: Nm,
    pub width: Nm,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for RoutePadPolyline {
    const NAME: &'static str = "route_pad_polyline";
    const DESCRIPTION: &'static str = "Atomically route between persisted explicit SMD pad IDs using exact integer quarter-turn centers, preserving off-grid endpoints and requiring grid-aligned supplied interior vertices. Resolve net/outer side from both pads; same assigned net/side required. Exact expected_project/initial expected_cursor required; stale, missing, arbitrary-angle or invalid endpoints reject all segments. Shared bounded scoped explicit-copper/outline preflight and one undo. No via routing, shove/walkaround, electrical continuity verification or full electrical connectivity certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.expected_project != project.id {
            return Err(CommandError::conflict("project changed; restart pad route"));
        }
        if self.interior.len() > 1023 {
            return Err(CommandError::invalid("interior", "must contain at most 1023 vertices"));
        }
        capture_budget(project)?;
        if self.start == self.target {
            return Err(CommandError::invalid("target", "start and target must be distinct explicit pads"));
        }
        let (start, start_layer, start_net) = resolve(project, self.start)?;
        let (target, target_layer, target_net) = resolve(project, self.target)?;
        if start_layer != target_layer {
            return Err(CommandError::invalid("target", "pad endpoints must share one outer copper side; routing vias is unsupported"));
        }
        if start_net != target_net {
            return Err(CommandError::invalid("target", "pad endpoints must share the exact assigned net"));
        }
        if start == target {
            return Err(CommandError::invalid("target", "endpoint centers coincide; no nonzero route can be added"));
        }
        let net = start_net.to_owned();
        let mut points = Vec::with_capacity(self.interior.len() + 2);
        points.push(start);
        points.extend(self.interior);
        points.push(target);
        RoutePolyline {
            expected_project: self.expected_project,
            expected_cursor: self.expected_cursor,
            points,
            grid: self.grid,
            width: self.width,
            layer: start_layer,
            net,
            rules: self.rules,
        }
        .apply_resolved(project, true)
    }
}
