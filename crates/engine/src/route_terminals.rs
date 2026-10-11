//! Stable existing copper terminals; identity resolution never rounds attachment coordinates.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor, PadEndpointId, RoutePolyline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{CopperLayer, ObjectId, Pad, Part, Project};
use viamind_geom::{Nm, Point};
use viamind_router::{CornerMode, Posture};

pub const MAX_ROUTE_TERMINAL_ROWS: usize = 100_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackEnd {
    Start,
    End,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteTerminalId {
    Pad { part_id: ObjectId, pad_id: ObjectId },
    TrackEnd { track_id: ObjectId, end: TrackEnd },
    Via { via_id: ObjectId },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteTerminal {
    pub id: RouteTerminalId,
    pub layer: CopperLayer,
    pub label: String,
    pub net: Option<String>,
    pub center_nm: Option<Point>,
    pub unsupported_reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteTerminalSnapshot {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub terminals: Vec<RouteTerminal>,
    pub omitted_legacy_parts: usize,
    pub unsupported_terminals: usize,
}
fn default_limit() -> usize {
    100
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouteTerminalQuery {
    pub net: Option<String>,
    pub layer: Option<CopperLayer>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteTerminalPage {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub terminals: Vec<RouteTerminal>,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub omitted_legacy_parts: usize,
    pub unsupported_terminals: usize,
}
fn pad_resolve<'a>(part: &Part, pad: &'a Pad) -> Result<(Point, CopperLayer, &'a str), CommandError> {
    if [pad.width.0, pad.height.0].iter().any(|s| *s <= 0 || *s > 10_000_000_000) {
        return Err(CommandError::invalid("terminal", "pad dimensions must be positive and within10000mm"));
    }
    crate::route_endpoints::resolve_pad(part, pad)
}
fn assigned(net: &Option<String>) -> Result<&str, CommandError> {
    net.as_deref()
        .filter(|n| !n.is_empty() && n.len() <= 256)
        .ok_or_else(|| CommandError::invalid("terminal", "terminal requires an assigned net within256bytes"))
}
pub(crate) fn resolve(project: &Project, id: RouteTerminalId, layer: CopperLayer) -> Result<(Point, &str), CommandError> {
    let (point, actual, net) = match id {
        RouteTerminalId::Pad { part_id, pad_id } => {
            let part = project.board.parts.get(&part_id).ok_or_else(|| CommandError::conflict("terminal part no longer exists"))?;
            let pad = part
                .pads
                .as_ref()
                .and_then(|p| p.0.iter().find(|p| p.id == pad_id))
                .ok_or_else(|| CommandError::conflict("terminal explicit pad no longer exists"))?;
            pad_resolve(part, pad)?
        }
        RouteTerminalId::TrackEnd { track_id, end } => {
            let t = project.board.tracks.get(&track_id).ok_or_else(|| CommandError::conflict("terminal track no longer exists"))?;
            t.validate().map_err(|e| CommandError::invalid("terminal", e))?;
            (
                match end {
                    TrackEnd::Start => t.start,
                    TrackEnd::End => t.end,
                },
                t.layer,
                assigned(&t.net)?,
            )
        }
        RouteTerminalId::Via { via_id } => {
            let v = project.board.vias.get(&via_id).ok_or_else(|| CommandError::conflict("terminal via no longer exists"))?;
            v.validate().map_err(|e| CommandError::invalid("terminal", e))?;
            (v.position, layer, assigned(&v.net)?)
        }
    };
    if actual != layer {
        return Err(CommandError::invalid("layer", "terminal does not exist on the selected outer copper layer"));
    }
    Ok((point, net))
}
fn budget(project: &Project) -> Result<usize, CommandError> {
    crate::route_endpoints::capture_budget(project)?;
    let mut rows = project.board.tracks.len().saturating_add(project.board.vias.len()).saturating_mul(2);
    if rows > MAX_ROUTE_TERMINAL_ROWS {
        return Err(CommandError::conflict("route terminal discovery exceeds100000rows"));
    }
    for part in project.board.parts.values() {
        rows = rows.saturating_add(part.pads.as_ref().map_or(0, |p| p.0.len()));
        if rows > MAX_ROUTE_TERMINAL_ROWS {
            return Err(CommandError::conflict("route terminal discovery exceeds100000rows"));
        }
    }
    Ok(rows)
}
impl EngineHandle {
    pub fn route_terminal_snapshot(&self) -> Result<RouteTerminalSnapshot, CommandError> {
        self.with_engine(|engine| {
            let project = engine.project();
            let rows = budget(project)?;
            let mut terminals = Vec::with_capacity(rows);
            let mut omitted_legacy_parts = 0;
            for part in project.board.parts.values() {
                let Some(pads) = &part.pads else {
                    omitted_legacy_parts += 1;
                    continue;
                };
                for pad in &pads.0 {
                    let id = RouteTerminalId::Pad { part_id: part.id, pad_id: pad.id };
                    let layer = match part.side {
                        viamind_doc::Side::Top => CopperLayer::Top,
                        viamind_doc::Side::Bottom => CopperLayer::Bottom,
                    };
                    let r = pad_resolve(part, pad);
                    terminals.push(RouteTerminal {
                        id,
                        layer,
                        label: format!("{} pad {}", part.designator, pad.number),
                        net: pad.net.clone(),
                        center_nm: r.as_ref().ok().map(|r| r.0),
                        unsupported_reason: r.err().map(|e| e.to_string()),
                    });
                }
            }
            for track in project.board.tracks.values() {
                for end in [TrackEnd::Start, TrackEnd::End] {
                    let id = RouteTerminalId::TrackEnd { track_id: track.id, end };
                    let r = resolve(project, id, track.layer);
                    terminals.push(RouteTerminal {
                        id,
                        layer: track.layer,
                        label: format!("Track {} {:?}", track.id, end),
                        net: track.net.clone(),
                        center_nm: r.as_ref().ok().map(|r| r.0),
                        unsupported_reason: r.err().map(|e| e.to_string()),
                    });
                }
            }
            for via in project.board.vias.values() {
                for layer in [CopperLayer::Top, CopperLayer::Bottom] {
                    let id = RouteTerminalId::Via { via_id: via.id };
                    let r = resolve(project, id, layer);
                    terminals.push(RouteTerminal {
                        id,
                        layer,
                        label: format!("Via {}", via.id),
                        net: via.net.clone(),
                        center_nm: r.as_ref().ok().map(|r| r.0),
                        unsupported_reason: r.err().map(|e| e.to_string()),
                    });
                }
            }
            terminals.sort_by_key(|t| {
                (
                    t.id,
                    match t.layer {
                        CopperLayer::Top => 0,
                        CopperLayer::Bottom => 1,
                    },
                )
            });
            let unsupported_terminals = terminals.iter().filter(|t| t.unsupported_reason.is_some()).count();
            Ok(RouteTerminalSnapshot {
                project_id: project.id,
                source_cursor: engine.event_cursor(),
                terminals,
                omitted_legacy_parts,
                unsupported_terminals,
            })
        })
    }
    pub fn query_route_terminals(&self, query: &RouteTerminalQuery) -> Result<RouteTerminalPage, CommandError> {
        if !(1..=1000).contains(&query.limit) {
            return Err(CommandError::invalid("limit", "must be within1..1000"));
        }
        if query.net.as_ref().is_some_and(|s| s.len() > 256) {
            return Err(CommandError::invalid("net", "must be within256bytes"));
        }
        let snapshot = self.route_terminal_snapshot()?;
        let matching: Vec<_> = snapshot
            .terminals
            .into_iter()
            .filter(|t| query.layer.is_none_or(|l| l == t.layer) && query.net.as_ref().is_none_or(|n| t.net.as_ref() == Some(n)))
            .collect();
        let total = matching.len();
        let unsupported_terminals = matching.iter().filter(|t| t.unsupported_reason.is_some()).count();
        let end = query.offset.saturating_add(query.limit);
        Ok(RouteTerminalPage {
            project_id: snapshot.project_id,
            source_cursor: snapshot.source_cursor,
            terminals: matching.into_iter().skip(query.offset).take(query.limit).collect(),
            total,
            next_offset: (end < total).then_some(end),
            omitted_legacy_parts: snapshot.omitted_legacy_parts,
            unsupported_terminals,
        })
    }
}
fn pair(
    project: &Project,
    expected: ObjectId,
    start: RouteTerminalId,
    target: RouteTerminalId,
    layer: CopperLayer,
) -> Result<(Point, Point, String), CommandError> {
    if project.id != expected {
        return Err(CommandError::conflict("project changed; restart terminal route"));
    }
    crate::route_endpoints::capture_budget(project)?;
    let (a, an) = resolve(project, start, layer)?;
    let (b, bn) = resolve(project, target, layer)?;
    if start == target || a == b || an != bn {
        return Err(CommandError::invalid("target", "choose distinct terminal positions on the selected layer with exactly the same assigned net"));
    }
    Ok((a, b, an.to_owned()))
}
fn bounded_points<'de, D: serde::Deserializer<'de>>(deserializer: D, maximum: usize) -> Result<Vec<Point>, D::Error> {
    struct Points(usize);
    impl<'de> serde::de::Visitor<'de> for Points {
        type Value = Vec<Point>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most{} integer points", self.0)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > self.0) {
                return Err(serde::de::Error::custom("too many route points"));
            }
            let mut points = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(self.0));
            while points.len() < self.0 {
                match seq.next_element()? {
                    Some(p) => points.push(p),
                    None => return Ok(points),
                }
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("too many route points"));
            }
            Ok(points)
        }
    }
    deserializer.deserialize_seq(Points(maximum))
}
fn interior<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Point>, D::Error> {
    bounded_points(d, 1023)
}
fn waypoints<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Point>, D::Error> {
    bounded_points(d, 510)
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouteTerminalPolyline {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: RouteTerminalId,
    pub target: RouteTerminalId,
    pub layer: CopperLayer,
    #[serde(default, deserialize_with = "interior")]
    #[schemars(length(max = 1023))]
    pub interior: Vec<Point>,
    pub grid: Nm,
    pub width: Nm,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for RouteTerminalPolyline {
    const NAME: &'static str = "route_terminal_polyline";
    const DESCRIPTION: &'static str = "Atomically route exact persisted pad, track start/end or existing through-via center identities on one explicit outer layer, deriving exact same assigned net. Positive explicit width/grid Nm; up to1023 grid-aligned interior points; exact off-grid terminal attachment. Full initial cursor/project guards, bounded board-only capture and complete shared scoped joint DRC; one undo. No midsegment split, new via, inferred size, obstacle avoidance/shove or physical continuity certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.interior.len() > 1023 {
            return Err(CommandError::invalid("interior", "at most1023 points"));
        }
        let (a, b, net) = pair(project, self.expected_project, self.start, self.target, self.layer)?;
        let mut points = Vec::with_capacity(self.interior.len() + 2);
        points.push(a);
        points.extend(self.interior);
        points.push(b);
        RoutePolyline {
            expected_project: self.expected_project,
            expected_cursor: self.expected_cursor,
            points,
            grid: self.grid,
            width: self.width,
            layer: self.layer,
            net,
            rules: self.rules,
        }
        .apply_resolved(project, true)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouteTerminalPosture {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub start: RouteTerminalId,
    pub target: RouteTerminalId,
    pub layer: CopperLayer,
    #[serde(default, deserialize_with = "waypoints")]
    #[schemars(length(max = 510))]
    pub waypoints: Vec<Point>,
    pub grid: Nm,
    pub width: Nm,
    pub mode: CornerMode,
    #[serde(default)]
    pub posture: Posture,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
impl Command for RouteTerminalPosture {
    const NAME: &'static str = "route_terminal_posture";
    const DESCRIPTION: &'static str = "Atomically route persisted pad, track endpoint or existing through-via center IDs using shared cardinal/octilinear posture math, explicit outer layer/width/grid and derived same assigned net. Up to510 grid-aligned waypoints, at most1024 segments; preserve exact off-grid terminals with explicit short nearest-grid stubs included in joint scoped DRC. Full project/initial cursor guards; one undo. No sizing inference, new via, midsegment splitting, obstacle avoidance/shove or physical certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if self.waypoints.len() > 510 {
            return Err(CommandError::invalid("waypoints", "at most510 points"));
        }
        let (a, b, net) = pair(project, self.expected_project, self.start, self.target, self.layer)?;
        let plan =
            viamind_router::plan_pad_path(a, b, &self.waypoints, self.grid, self.mode, self.posture).map_err(|e| CommandError::invalid("path", e))?;
        RoutePolyline {
            expected_project: self.expected_project,
            expected_cursor: self.expected_cursor,
            points: plan.points,
            grid: self.grid,
            width: self.width,
            layer: self.layer,
            net,
            rules: self.rules,
        }
        .apply_resolved(project, true)
    }
}
impl From<PadEndpointId> for RouteTerminalId {
    fn from(id: PadEndpointId) -> Self {
        Self::Pad { part_id: id.part_id, pad_id: id.pad_id }
    }
}
