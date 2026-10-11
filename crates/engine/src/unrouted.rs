//! Whole-inventory terminal connectivity in a bounded exact planar copper subset.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Side};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};

use crate::{CommandError, EngineHandle, EventCursor, copper_connectivity::Forest};

const MAX_OBJECTS: usize = 100_000;
const MAX_NETS: usize = 10_000;
const MAX_TEXT: usize = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_CANDIDATES: usize = 1_000_000;
const COORD: u64 = 1_000_000_000;
const SIZE: u64 = 100_000_000;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Strict parameterless observation; no pagination or caller-weakened limits.
#[derive(Clone, Copy, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnroutedNetQuery {}
impl<'de> Deserialize<'de> for UnroutedNetQuery {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EmptyObject;
        impl<'de> serde::de::Visitor<'de> for EmptyObject {
            type Value = UnroutedNetQuery;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an empty JSON object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                if map.next_key::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::custom("unrouted-net query accepts no fields"));
                }
                Ok(UnroutedNetQuery {})
            }
        }
        deserializer.deserialize_map(EmptyObject)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnroutedNetStatus {
    Connected,
    Disconnected,
    InsufficientTerminals,
    Unknown,
}
/// Fixed reasons, never copied user descriptions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnroutedNetReason {
    UnsupportedPad,
    UnsupportedTrack,
    UnsupportedVia,
    OverlappingDrill,
    ForeignDrill,
    DrilledAway,
    UnlocatedDrill,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnroutedNet {
    pub name: String,
    pub terminals: usize,
    pub tracks: usize,
    pub vias: usize,
    pub terminal_islands: Option<usize>,
    pub status: UnroutedNetStatus,
    pub unrouted: Option<bool>,
    pub reasons: Vec<UnroutedNetReason>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnroutedCoverage {
    pub supported_pads: usize,
    pub supported_tracks: usize,
    pub supported_vias: usize,
    pub unsupported_objects: usize,
    pub unassigned_objects: usize,
    pub excluded_legacy_parts: usize,
    pub candidate_visits: usize,
}

/// Complete bounded net inventory, distinct from a physical continuity certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnroutedNetResult {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub complete: bool,
    pub model: String,
    pub coverage: UnroutedCoverage,
    pub connected_count: usize,
    pub unrouted_count: usize,
    pub insufficient_terminal_count: usize,
    pub unknown_count: usize,
    pub exclusions: Vec<String>,
    pub nets: Vec<UnroutedNet>,
}

struct Permit;
impl Permit {
    fn reserve() -> Result<Self, CommandError> {
        ACTIVE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < 2).then_some(n + 1))
            .map(|_| Self)
            .map_err(|_| error("busy: two unrouted-net workers are unfinished"))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Admit before spawning; move this owned work into the actual retained worker.
pub struct UnroutedNetWork {
    engine: EngineHandle,
    _permit: Permit,
}
impl EngineHandle {
    pub fn prepare_unrouted_nets(&self, _query: UnroutedNetQuery) -> Result<UnroutedNetWork, CommandError> {
        Ok(UnroutedNetWork { engine: self.clone(), _permit: Permit::reserve()? })
    }
}
impl UnroutedNetWork {
    /// Capture under lock, analyze outside, and preflight the entire serialization.
    pub fn run(self) -> Result<UnroutedNetResult, CommandError> {
        let (result, _) = self.execute()?;
        Ok(result)
    }
    /// MCP's bounded JSON bridge is built while the actual worker owns admission.
    pub fn run_value(self) -> Result<serde_json::Value, CommandError> {
        let (_, bytes) = self.execute()?;
        serde_json::from_slice(&bytes).map_err(|e| error(format!("result JSON: {e}")))
    }
    fn execute(&self) -> Result<(UnroutedNetResult, Vec<u8>), CommandError> {
        let snapshot = self.engine.with_engine(|e| Snapshot::capture(e.project(), e.event_cursor()))?;
        let result = snapshot.analyze(MAX_CANDIDATES)?;
        let bytes = serialize(&result, MAX_OUTPUT)?;
        Ok((result, bytes))
    }
}
fn error(message: impl Into<String>) -> CommandError {
    CommandError::invalid("unrouted_nets", message)
}
struct LimitedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|n| n > self.maximum) {
            return Err(io::Error::other("whole-query output budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn serialize(result: &UnroutedNetResult, maximum: usize) -> Result<Vec<u8>, CommandError> {
    let mut writer = LimitedWriter { bytes: Vec::new(), maximum };
    serde_json::to_writer(&mut writer, result).map_err(|e| error(format!("whole-query output refused: {e}")))?;
    Ok(writer.bytes)
}

/// Doubled integer Nm: rectangle edges and radii preserve odd dimensions exactly.
type P = [i64; 2];
#[derive(Clone, Copy, Debug)]
enum Shape {
    Rect { min: P, max: P },
    Capsule { a: P, b: P, radius: i64 },
    Ring { center: P, inner: i64, outer: i64 },
}
fn squared(a: P, b: P) -> i128 {
    let d = [i128::from(a[0]) - i128::from(b[0]), i128::from(a[1]) - i128::from(b[1])];
    // Inputs are bounded to +/-2.1e9 doubled Nm; these products are <4e19.
    d[0] * d[0] + d[1] * d[1]
}
fn square(n: i64) -> i128 {
    i128::from(n) * i128::from(n)
}
fn interval_gap(a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.0 - b.1).max(b.0 - a.1).max(0)
}
fn segment_point(a: P, b: P, point: P) -> i128 {
    squared(point, [point[0].clamp(a[0].min(b[0]), a[0].max(b[0])), point[1].clamp(a[1].min(b[1]), a[1].max(b[1]))])
}
fn core_rect(a: P, b: P, min: P, max: P) -> i128 {
    square(interval_gap((a[0].min(b[0]), a[0].max(b[0])), (min[0], max[0])))
        + square(interval_gap((a[1].min(b[1]), a[1].max(b[1])), (min[1], max[1])))
}
impl Shape {
    fn bbox(self) -> Rect {
        let (min, max) = match self {
            Self::Rect { min, max } => (min, max),
            Self::Capsule { a, b, radius: r } => ([a[0].min(b[0]) - r, a[1].min(b[1]) - r], [a[0].max(b[0]) + r, a[1].max(b[1]) + r]),
            Self::Ring { center: c, outer: r, .. } => ([c[0] - r, c[1] - r], [c[0] + r, c[1] + r]),
        };
        Rect::new(Point::new(Nm(min[0]), Nm(min[1])), Point::new(Nm(max[0]), Nm(max[1])))
    }
    /// Intersects a closed disk, using exact squared comparisons.
    fn disk(self, center: P, radius: i64) -> bool {
        match self {
            Self::Rect { min, max } => core_rect(center, center, min, max) <= square(radius),
            Self::Capsule { a, b, radius: r } => segment_point(a, b, center) <= square(radius + r),
            Self::Ring { center: c, inner, outer } => {
                let d = squared(c, center);
                d <= square(outer + radius) && d >= square((inner - radius).max(0))
            }
        }
    }
    /// Has any material on or outside the drill boundary.
    fn reaches_outside(self, center: P, radius: i64) -> bool {
        match self {
            Self::Rect { min, max } => [min, max, [min[0], max[1]], [max[0], min[1]]].into_iter().any(|p| squared(p, center) >= square(radius)),
            Self::Capsule { a, b, radius: r } => squared(a, center).max(squared(b, center)) >= square((radius - r).max(0)),
            Self::Ring { center: c, outer, .. } => squared(c, center) >= square((radius - outer).max(0)),
        }
    }
    fn touches(self, other: Self) -> bool {
        match (self, other) {
            (Self::Ring { center, inner, outer }, shape) | (shape, Self::Ring { center, inner, outer }) => {
                shape.disk(center, outer) && shape.reaches_outside(center, inner)
            }
            (Self::Rect { min: a, max: b }, Self::Rect { min: c, max: d }) => {
                interval_gap((a[0], b[0]), (c[0], d[0])) == 0 && interval_gap((a[1], b[1]), (c[1], d[1])) == 0
            }
            (Self::Capsule { a, b, radius }, Self::Rect { min, max }) | (Self::Rect { min, max }, Self::Capsule { a, b, radius }) => {
                core_rect(a, b, min, max) <= square(radius)
            }
            (Self::Capsule { a, b, radius: r }, Self::Capsule { a: c, b: d, radius: s }) => {
                core_rect(a, b, [c[0].min(d[0]), c[1].min(d[1])], [c[0].max(d[0]), c[1].max(d[1])]) <= square(r + s)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Pad,
    Track,
    Via,
}
struct Object {
    net: Option<usize>,
    kind: Kind,
    layers: u8,
    shape: Option<Shape>,
}
struct Net {
    name: String,
    terminals: usize,
    tracks: usize,
    vias: usize,
    reasons: BTreeSet<UnroutedNetReason>,
}
struct Snapshot {
    project_id: ObjectId,
    cursor: EventCursor,
    nets: Vec<Net>,
    objects: Vec<Object>,
    coverage: UnroutedCoverage,
    unlocated_drill: bool,
}
fn name_budget(name: &str, text: &mut usize) -> Result<(), CommandError> {
    if name.is_empty() || name.trim() != name || name.len() > 512 || name.chars().count() > 128 || name.chars().any(char::is_control) {
        return Err(error("assigned/defined names must be trimmed, nonempty, control-free, <=128 characters/512 bytes"));
    }
    *text = text.checked_add(name.len()).filter(|n| *n <= MAX_TEXT).ok_or_else(|| error("whole-query net text budget exceeded"))?;
    Ok(())
}
fn coordinate(p: Point) -> Option<P> {
    [p.x.0, p.y.0].iter().all(|v| v.unsigned_abs() <= COORD).then(|| [p.x.0 * 2, p.y.0 * 2])
}
fn length(n: Nm) -> bool {
    n.0 > 0 && n.0.unsigned_abs() <= SIZE
}
fn layer(side: Side) -> u8 {
    if side == Side::Top { 1 } else { 2 }
}
impl Snapshot {
    fn capture(project: &viamind_doc::Project, cursor: EventCursor) -> Result<Self, CommandError> {
        if project.id.as_uuid().is_nil() {
            return Err(error("project identity must be nonnil"));
        }
        let b = &project.board;
        let mut count = b
            .parts
            .len()
            .checked_add(b.tracks.len())
            .and_then(|n| n.checked_add(b.vias.len()))
            .and_then(|n| n.checked_add(b.net_rules.len()))
            .filter(|n| *n <= MAX_OBJECTS)
            .ok_or_else(|| error("whole-query object budget exceeded"))?;
        let mut text = 0;
        for part in b.parts.values() {
            if let Some(pads) = &part.pads {
                count = count.checked_add(pads.0.len()).filter(|n| *n <= MAX_OBJECTS).ok_or_else(|| error("whole-query object budget exceeded"))?;
                for pad in &pads.0 {
                    if let Some(net) = &pad.net {
                        name_budget(net, &mut text)?;
                    }
                }
            }
        }
        for name in b
            .tracks
            .values()
            .filter_map(|t| t.net.as_deref())
            .chain(b.vias.values().filter_map(|v| v.net.as_deref()))
            .chain(b.net_rules.keys().map(String::as_str))
        {
            name_budget(name, &mut text)?;
        }
        // Only after the aggregate borrowed preflight: bounded identity/name indices.
        let mut names = BTreeSet::new();
        for name in b
            .parts
            .values()
            .filter_map(|p| p.pads.as_ref())
            .flat_map(|p| &p.0)
            .filter_map(|p| p.net.as_deref())
            .chain(b.tracks.values().filter_map(|t| t.net.as_deref()))
            .chain(b.vias.values().filter_map(|v| v.net.as_deref()))
            .chain(b.net_rules.keys().map(String::as_str))
        {
            names.insert(name);
            if names.len() > MAX_NETS {
                return Err(error("whole-query distinct net budget exceeded"));
            }
        }
        let lookup: BTreeMap<_, _> = names.into_iter().enumerate().map(|(i, name)| (name, i)).collect();
        let mut result = Self {
            project_id: project.id,
            cursor,
            nets: lookup.keys().map(|name| Net { name: (*name).into(), terminals: 0, tracks: 0, vias: 0, reasons: BTreeSet::new() }).collect(),
            objects: Vec::new(),
            coverage: UnroutedCoverage::default(),
            unlocated_drill: false,
        };
        let mut ids = BTreeSet::new();
        let mut identity = |id: ObjectId, key: ObjectId| -> Result<(), CommandError> {
            if id != key || id.as_uuid().is_nil() || !ids.insert(id) {
                return Err(error("malformed or duplicate copper identity"));
            }
            Ok(())
        };
        for (key, part) in &b.parts {
            identity(part.id, *key)?;
            let Some(pads) = &part.pads else {
                result.coverage.excluded_legacy_parts += 1;
                continue;
            };
            for pad in &pads.0 {
                identity(pad.id, pad.id)?;
                let shape = part
                    .local_to_world_nm_exact(pad.position)
                    .ok()
                    .and_then(coordinate)
                    .filter(|_| (0..360_000).contains(&part.rotation_mdeg) && length(pad.width) && length(pad.height))
                    .map(|center| {
                        let (w, h) =
                            if part.rotation_mdeg.rem_euclid(180_000) == 90_000 { (pad.height.0, pad.width.0) } else { (pad.width.0, pad.height.0) };
                        Shape::Rect { min: [center[0] - w, center[1] - h], max: [center[0] + w, center[1] + h] }
                    });
                result.push(Object { net: pad.net.as_deref().map(|n| lookup[n]), kind: Kind::Pad, layers: layer(part.side), shape });
            }
        }
        for (key, track) in &b.tracks {
            identity(track.id, *key)?;
            let shape = coordinate(track.start)
                .zip(coordinate(track.end))
                .filter(|(a, b)| a != b && (a[0] == b[0] || a[1] == b[1]) && length(track.width))
                .map(|(a, b)| Shape::Capsule { a, b, radius: track.width.0 });
            result.push(Object {
                net: track.net.as_deref().map(|n| lookup[n]),
                kind: Kind::Track,
                layers: if track.layer == viamind_doc::CopperLayer::Top { 1 } else { 2 },
                shape,
            });
        }
        for (key, via) in &b.vias {
            identity(via.id, *key)?;
            let shape = coordinate(via.position)
                .filter(|_| length(via.diameter) && length(via.drill) && via.drill < via.diameter)
                .map(|center| Shape::Ring { center, inner: via.drill.0, outer: via.diameter.0 });
            result.unlocated_drill |= shape.is_none();
            result.push(Object { net: via.net.as_deref().map(|n| lookup[n]), kind: Kind::Via, layers: 3, shape });
        }
        Ok(result)
    }
    fn push(&mut self, object: Object) {
        if let Some(net) = object.net {
            let row = &mut self.nets[net];
            match object.kind {
                Kind::Pad => row.terminals += 1,
                Kind::Track => row.tracks += 1,
                Kind::Via => row.vias += 1,
            }
            if object.shape.is_none() {
                row.reasons.insert(match object.kind {
                    Kind::Pad => UnroutedNetReason::UnsupportedPad,
                    Kind::Track => UnroutedNetReason::UnsupportedTrack,
                    Kind::Via => UnroutedNetReason::UnsupportedVia,
                });
            }
        } else {
            self.coverage.unassigned_objects += 1;
        }
        match (object.kind, object.shape.is_some()) {
            (_, false) => self.coverage.unsupported_objects += 1,
            (Kind::Pad, true) => self.coverage.supported_pads += 1,
            (Kind::Track, true) => self.coverage.supported_tracks += 1,
            (Kind::Via, true) => self.coverage.supported_vias += 1,
        }
        self.objects.push(object);
    }
    fn analyze(mut self, max_candidates: usize) -> Result<UnroutedNetResult, CommandError> {
        let mut forest = Forest::with_size(self.objects.len());
        let index = SpatialIndex::bulk_load(self.objects.iter().enumerate().filter_map(|(i, o)| o.shape.map(|s| (i, s.bbox()))));
        let mut visits = 0usize;
        let mut visit = || -> Result<(), CommandError> {
            visits =
                visits.checked_add(1).filter(|n| *n <= max_candidates).ok_or_else(|| error("whole-query contact/hole candidate budget exceeded"))?;
            Ok(())
        };
        if self.unlocated_drill {
            for net in &mut self.nets {
                net.reasons.insert(UnroutedNetReason::UnlocatedDrill);
            }
        }
        // Validate drill effects before any contact can be interpreted as complete.
        for (i, hole) in self.objects.iter().enumerate().filter(|(_, o)| o.kind == Kind::Via) {
            let Some(Shape::Ring { center, inner, .. }) = hole.shape else { continue };
            let disk = Shape::Ring { center, inner: 0, outer: inner };
            for (j, _) in index.query_rect(&disk.bbox()) {
                if i == j {
                    continue;
                }
                visit()?;
                let other = &self.objects[j];
                let shape = other.shape.expect("indexed supported shape");
                // Other annuli must remain intact: compare their OUTER disk to the hole.
                let hits = match shape {
                    Shape::Ring { center: c, outer, .. } => squared(c, center) <= square(outer + inner),
                    _ => shape.disk(center, inner),
                };
                if !hits {
                    continue;
                }
                let reason = if other.kind == Kind::Via {
                    Some(UnroutedNetReason::OverlappingDrill)
                } else if other.net.is_none() || other.net != hole.net {
                    Some(UnroutedNetReason::ForeignDrill)
                } else if !shape.reaches_outside(center, inner) || !shape.touches(hole.shape.expect("valid via")) {
                    Some(UnroutedNetReason::DrilledAway)
                } else {
                    None
                };
                if let Some(reason) = reason
                    && let Some(net) = other.net
                {
                    self.nets[net].reasons.insert(reason);
                }
            }
        }
        for (i, object) in self.objects.iter().enumerate() {
            let (Some(net), Some(shape)) = (object.net, object.shape) else { continue };
            for (j, _) in index.query_rect(&shape.bbox()) {
                if j <= i {
                    continue;
                }
                visit()?;
                let other = &self.objects[j];
                if other.net == Some(net) && other.layers & object.layers != 0 && shape.touches(other.shape.expect("indexed shape")) {
                    forest.join(i, j);
                }
            }
        }
        self.coverage.candidate_visits = visits;
        let mut roots: Vec<BTreeSet<usize>> = (0..self.nets.len()).map(|_| BTreeSet::new()).collect();
        for (i, object) in self.objects.iter().enumerate().filter(|(_, o)| o.kind == Kind::Pad) {
            if let Some(net) = object.net {
                roots[net].insert(forest.root(i));
            }
        }
        let mut result = UnroutedNetResult {
            project_id: self.project_id,
            source_cursor: self.cursor,
            complete: self.coverage.excluded_legacy_parts == 0,
            model: "exact_cardinal_outer_copper".into(),
            coverage: self.coverage,
            connected_count: 0,
            unrouted_count: 0,
            insufficient_terminal_count: 0,
            unknown_count: 0,
            exclusions: vec![
                "Schematic obligations, legacy previews, inner copper, pours, arcs and plated pad holes are not analyzed.".into(),
                "Terminal connectivity is not a dangling-copper, short-circuit, DRC, manufacturing or physical continuity certificate.".into(),
            ],
            nets: Vec::new(),
        };
        for (net, roots) in self.nets.into_iter().zip(roots) {
            let (status, unrouted, islands) = if !net.reasons.is_empty() {
                result.unknown_count += 1;
                result.complete = false;
                (UnroutedNetStatus::Unknown, None, None)
            } else if net.terminals < 2 {
                result.insufficient_terminal_count += 1;
                (UnroutedNetStatus::InsufficientTerminals, Some(false), Some(roots.len()))
            } else if roots.len() == 1 {
                result.connected_count += 1;
                (UnroutedNetStatus::Connected, Some(false), Some(1))
            } else {
                result.unrouted_count += 1;
                (UnroutedNetStatus::Disconnected, Some(true), Some(roots.len()))
            };
            result.nets.push(UnroutedNet {
                name: net.name,
                terminals: net.terminals,
                tracks: net.tracks,
                vias: net.vias,
                terminal_islands: islands,
                status,
                unrouted,
                reasons: net.reasons.into_iter().collect(),
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use viamind_doc::{CopperLayer, Pad, Pads, Part, Project, Track, Via};
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }
    fn pad(project: &mut Project, x: i64, y: i64, side: Side, name: &str, width: i64, height: i64) -> ObjectId {
        let pad =
            Pad { id: ObjectId::new(), number: "1".into(), position: Point::ORIGIN, width: Nm(width), height: Nm(height), net: Some(name.into()) };
        let part = Part {
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            pads: Some(Pads(vec![pad])),
            id: ObjectId::new(),
            designator: String::new(),
            value: String::new(),
            footprint: String::new(),
            position: p(x, y),
            rotation_mdeg: 0,
            side,
        };
        let id = part.id;
        project.board.parts.insert(id, part);
        id
    }
    fn track(project: &mut Project, a: (i64, i64), b: (i64, i64), side: CopperLayer, name: &str, width: i64) -> ObjectId {
        let track = Track { id: ObjectId::new(), start: p(a.0, a.1), end: p(b.0, b.1), width: Nm(width), layer: side, net: Some(name.into()) };
        let id = track.id;
        project.board.tracks.insert(id, track);
        id
    }
    fn via(project: &mut Project, x: i64, y: i64, name: Option<&str>, diameter: i64, drill: i64) -> ObjectId {
        let via = Via { id: ObjectId::new(), position: p(x, y), diameter: Nm(diameter), drill: Nm(drill), net: name.map(str::to_owned) };
        let id = via.id;
        project.board.vias.insert(id, via);
        id
    }
    fn analyze(project: &Project) -> UnroutedNetResult {
        Snapshot::capture(project, EventCursor { stream_id: ObjectId::new(), sequence: 7 }).unwrap().analyze(MAX_CANDIDATES).unwrap()
    }
    fn single(project: &Project) -> UnroutedNet {
        let mut result = analyze(project);
        assert_eq!(result.nets.len(), 1);
        result.nets.remove(0)
    }
    #[test]
    fn interior_crossings_pad_edges_width_contacts_and_exact_gaps_have_independent_oracles() {
        for mode in 0..6 {
            let mut project = Project::new("Actual filled copper, not guides");
            pad(&mut project, -100, 0, Side::Top, "N", 10, 10);
            pad(&mut project, 0, 100, Side::Top, "N", 10, 10);
            track(&mut project, (-100, 0), (100, 0), CopperLayer::Top, "N", 10);
            match mode {
                0 => {
                    track(&mut project, (0, -100), (0, 100), CopperLayer::Top, "N", 10);
                } // Interior crossing.
                1 => {
                    track(&mut project, (0, 10), (0, 100), CopperLayer::Top, "N", 10);
                } // Both 5Nm caps touch.
                2 => {
                    track(&mut project, (0, 11), (0, 100), CopperLayer::Top, "N", 10);
                } // Exact 1Nm gap.
                3 => {
                    track(&mut project, (0, 0), (0, 100), CopperLayer::Bottom, "N", 10);
                }
                4 => {
                    track(&mut project, (0, 0), (0, 100), CopperLayer::Top, "other", 10);
                }
                _ => {
                    track(&mut project, (0, 0), (1, 100), CopperLayer::Top, "N", 10);
                }
            }
            let result = analyze(&project);
            let row = result.nets.iter().find(|n| n.name == "N").unwrap();
            assert_eq!(
                row.status,
                if mode < 2 {
                    UnroutedNetStatus::Connected
                } else if mode == 5 {
                    UnroutedNetStatus::Unknown
                } else {
                    UnroutedNetStatus::Disconnected
                },
                "mode{mode}"
            );
        }
        for gap in [0, 1] {
            let mut project = Project::new("Rect edge, no center attachment");
            pad(&mut project, 0, 0, Side::Top, "N", 21, 11);
            pad(&mut project, 100, 0, Side::Top, "N", 11, 11);
            track(&mut project, (11 + gap, 0), (94, 0), CopperLayer::Top, "N", 1);
            // First pad ends at10.5Nm; radius0.5 reaches it only at x11.
            // Second pad starts94.5Nm; track end94 +0.5 reaches it.
            assert_eq!(single(&project).unrouted, Some(gap != 0));
        }
    }
    #[test]
    fn mirrored_quarter_turn_rectangles_keep_half_nm_extents_without_rounding() {
        let mut project = Project::new("Asymmetric bottom pad");
        let id = pad(&mut project, 100, 200, Side::Bottom, "N", 11, 21);
        let part = project.board.parts.get_mut(&id).unwrap();
        part.rotation_mdeg = 90_000;
        part.pads.as_mut().unwrap().0[0].position = p(3, 7);
        // Bottom mirror (-3,7), quarter turn (7,3), translation: (107,203).
        // Width21/height11 -> doubled rectangle [193,395]..[235,417].
        let snapshot = Snapshot::capture(&project, EventCursor { stream_id: ObjectId::new(), sequence: 0 }).unwrap();
        let Some(Shape::Rect { min, max }) = snapshot.objects[0].shape else { panic!("rectangle") };
        assert_eq!((min, max), ([193, 395], [235, 417]));
        pad(&mut project, 128, 203, Side::Bottom, "N", 21, 11);
        assert_eq!(single(&project).status, UnroutedNetStatus::Connected);
        project.board.parts.get_mut(&id).unwrap().rotation_mdeg = 90_001;
        assert_eq!(single(&project).reasons, vec![UnroutedNetReason::UnsupportedPad]);
    }
    #[test]
    fn annulus_interior_contacts_bridge_layers_and_drill_center_never_counts_as_copper() {
        for mode in 0..5 {
            let mut project = Project::new("Actual annulus radial interval");
            pad(&mut project, -100, 0, Side::Top, "N", 10, 10);
            pad(&mut project, 100, 0, Side::Bottom, "N", 10, 10);
            track(&mut project, (-100, 0), (0, 0), CopperLayer::Top, "N", 10);
            track(&mut project, (0, 0), (100, 0), CopperLayer::Bottom, "N", 10);
            let id = via(&mut project, 0, 0, Some("N"), 40, 20);
            if mode == 1 {
                project.board.vias.get_mut(&id).unwrap().net = Some("foreign".into());
            }
            if mode == 2 {
                project.board.vias.get_mut(&id).unwrap().net = None;
            }
            if mode == 3 {
                project.board.vias.get_mut(&id).unwrap().drill = Nm(40);
            }
            if mode == 4 {
                via(&mut project, 25, 0, Some("foreign"), 20, 12);
            }
            let result = analyze(&project);
            let row = result.nets.iter().find(|n| n.name == "N").unwrap();
            assert_eq!(row.status, if mode == 0 { UnroutedNetStatus::Connected } else { UnroutedNetStatus::Unknown }, "mode{mode}");
        }
        let mut project = Project::new("Pad occupies annulus only, no center match");
        pad(&mut project, 16, 0, Side::Top, "N", 4, 4);
        pad(&mut project, -16, 0, Side::Bottom, "N", 4, 4);
        via(&mut project, 0, 0, Some("N"), 40, 20);
        assert_eq!(single(&project).status, UnroutedNetStatus::Connected);
        let id = pad(&mut project, 0, 0, Side::Top, "N", 4, 4);
        assert_eq!(single(&project).status, UnroutedNetStatus::Unknown);
        project.board.parts.remove(&id);
        let id = via(&mut project, 0, 0, Some("N"), 8, 4);
        assert_eq!(single(&project).status, UnroutedNetStatus::Unknown, "nested via rings and drill effects cannot imply connected");
        project.board.vias.remove(&id);
        assert_eq!(single(&project).status, UnroutedNetStatus::Connected);
    }
    #[test]
    fn annulus_pair_and_radial_interval_integer_predicates_exclude_outer_disk_false_joins() {
        let ring = Shape::Ring { center: [0, 0], inner: 20, outer: 40 };
        let small = Shape::Ring { center: [0, 0], inner: 2, outer: 8 };
        assert!(!ring.touches(small));
        assert!(!small.touches(ring));
        assert!(ring.touches(Shape::Ring { center: [70, 0], inner: 20, outer: 40 }));
        assert!(!ring.touches(Shape::Ring { center: [81, 0], inner: 20, outer: 40 }));
        assert!(!ring.touches(Shape::Rect { min: [-5, -5], max: [5, 5] }));
        assert!(ring.touches(Shape::Rect { min: [19, -1], max: [21, 1] }));
        assert!(ring.touches(Shape::Capsule { a: [-100, 0], b: [100, 0], radius: 1 }));
        assert!(!ring.touches(Shape::Capsule { a: [-5, 0], b: [5, 0], radius: 1 }));
        for shape in [small, Shape::Rect { min: [19, -1], max: [21, 1] }, Shape::Capsule { a: [-100, 0], b: [100, 0], radius: 1 }] {
            assert_eq!(ring.touches(shape), shape.touches(ring));
        }
    }
    #[test]
    fn complete_inventory_exceeds_old_pages_and_includes_zero_one_and_copper_only_definitions() {
        let mut project = Project::new("One call, no guide generation");
        for i in 0..1201 {
            let name = format!("N{i:04}");
            pad(&mut project, i * 100, 0, Side::Top, &name, 10, 10);
            pad(&mut project, i * 100, 20, Side::Top, &name, 10, 10);
        }
        project.board.net_rules.insert(
            "RULE_ONLY".into(),
            viamind_doc::NamedNetMinima {
                minimum_copper_clearance: Nm(0),
                minimum_track_width: Nm(1),
                minimum_via_drill: Nm(0),
                minimum_annular_width: Nm(0),
            },
        );
        track(&mut project, (0, 200), (100, 200), CopperLayer::Top, "COPPER_ONLY", 10);
        pad(&mut project, 0, 300, Side::Top, "ONE", 10, 10);
        let result = analyze(&project);
        assert!(result.complete);
        assert_eq!(result.unrouted_count, 1201);
        assert_eq!(result.insufficient_terminal_count, 3);
        assert_eq!(result.nets.len(), 1204);
        assert!(result.nets.windows(2).all(|rows| rows[0].name < rows[1].name));
        assert_eq!(result.nets.iter().find(|r| r.name == "RULE_ONLY").unwrap().terminal_islands, Some(0));
        assert_eq!(result.nets.iter().find(|r| r.name == "ONE").unwrap().terminal_islands, Some(1));
        let expected = serde_json::to_value(&result.nets).unwrap();
        assert_eq!(expected, serde_json::to_value(analyze(&project).nets).unwrap());
    }
    #[test]
    fn purpose_capture_excludes_unused_poison_and_live_observation_preserves_complete_state() {
        let _serial = TEST_LOCK.lock().unwrap();
        let mut project = Project::new("Unused huge fields");
        let id = pad(&mut project, 0, 0, Side::Top, "N", 10, 10);
        pad(&mut project, 100, 0, Side::Top, "N", 10, 10);
        project.board.parts.get_mut(&id).unwrap().value = "x".repeat(3 * 1024 * 1024);
        project.board.parts.get_mut(&id).unwrap().assembly =
            Some(Box::new(viamind_doc::AssemblyMetadata { manufacturer: Some("invalid unused metadata\n".repeat(200_000)), ..Default::default() }));
        let mut sheet = viamind_doc::Sheet::new("x".repeat(3 * 1024 * 1024));
        sheet.id = "00000000-0000-0000-0000-000000000000".parse().unwrap();
        project.schematic.sheets.insert(sheet.id, sheet);
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let status = e.document_status();
        let drc = e.auto_drc_event_state();
        let events = e.subscribe();
        let result = e.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap().run().unwrap();
        assert_eq!(result.source_cursor, before.cursor);
        assert_eq!(result.project_id, before.project.id);
        assert_eq!(result.unrouted_count, 1);
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
        assert_eq!(e.auto_drc_event_state(), drc);
        assert!(events.try_recv().is_err());
    }
    #[test]
    fn budget_identity_name_and_candidate_failures_refuse_whole_results() {
        let mut project = Project::new("Strict complete-result admission");
        let id = pad(&mut project, 0, 0, Side::Top, "N", 10, 10);
        pad(&mut project, 1, 0, Side::Top, "N", 10, 10);
        let cursor = EventCursor { stream_id: ObjectId::new(), sequence: 3 };
        assert!(Snapshot::capture(&project, cursor).unwrap().analyze(0).is_err());
        let good = analyze(&project);
        assert!(serialize(&good, 10).is_err());
        assert_eq!(serde_json::from_slice::<UnroutedNetResult>(&serialize(&good, MAX_OUTPUT).unwrap()).unwrap(), good);
        let part = project.board.parts.get_mut(&id).unwrap();
        part.pads.as_mut().unwrap().0[0].net = Some("x".repeat(513));
        assert!(Snapshot::capture(&project, cursor).is_err());
        project.board.parts.get_mut(&id).unwrap().pads.as_mut().unwrap().0[0].net = Some("N".into());
        project.board.parts.get_mut(&id).unwrap().id = ObjectId::new();
        assert!(Snapshot::capture(&project, cursor).is_err());
        let mut text = MAX_TEXT;
        assert!(name_budget("N", &mut text).is_err());
        let mut text = usize::MAX;
        assert!(name_budget("N", &mut text).is_err());
        for input in [serde_json::json!({"limit":1}), serde_json::json!([]), serde_json::json!(null), serde_json::json!(0), serde_json::json!("")] {
            assert!(serde_json::from_value::<UnroutedNetQuery>(input).is_err());
        }
        assert!(serde_json::from_value::<UnroutedNetQuery>(serde_json::json!({})).is_ok());
    }
    #[test]
    fn unsupported_scalars_and_legacy_parts_are_explicit_incomplete_coverage() {
        let mut project = Project::new("Unsupported without losing inventory");
        let id = pad(&mut project, 0, 0, Side::Top, "N", 10, 10);
        project.board.parts.get_mut(&id).unwrap().position = p(i64::MAX, i64::MIN);
        let row = single(&project);
        assert_eq!(row.status, UnroutedNetStatus::Unknown);
        project.board.parts.get_mut(&id).unwrap().pads = None;
        let result = analyze(&project);
        assert!(!result.complete);
        assert_eq!(result.coverage.excluded_legacy_parts, 1);
        project.board.parts.get_mut(&id).unwrap().pads = Some(Pads(vec![]));
        assert!(analyze(&project).complete);
    }
    #[test]
    fn aggregate_object_text_and_distinct_net_caps_reject_before_secondary_geometry_copies() {
        let mut project = Project::new("Pre-copy object and name admission");
        let id = pad(&mut project, 0, 0, Side::Top, "N", 10, 10);
        let original = project.board.parts[&id].pads.as_ref().unwrap().0[0].clone();
        let cursor = EventCursor { stream_id: ObjectId::new(), sequence: 0 };
        project.board.parts.get_mut(&id).unwrap().pads = Some(Pads(vec![original.clone(); MAX_OBJECTS + 1]));
        assert!(Snapshot::capture(&project, cursor).err().unwrap().to_string().contains("object budget"));
        let mut huge_name = original.clone();
        huge_name.net = Some("界".repeat(128));
        project.board.parts.get_mut(&id).unwrap().pads = Some(Pads(vec![huge_name; 6000]));
        assert!(Snapshot::capture(&project, cursor).err().unwrap().to_string().contains("text budget"));
        project.board.parts.get_mut(&id).unwrap().pads = Some(Pads(vec![original]));
        let rules = viamind_doc::NamedNetMinima {
            minimum_copper_clearance: Nm(0),
            minimum_track_width: Nm(1),
            minimum_via_drill: Nm(0),
            minimum_annular_width: Nm(0),
        };
        for i in 0..MAX_NETS {
            project.board.net_rules.insert(format!("NET{i:05}"), rules);
        }
        assert!(Snapshot::capture(&project, cursor).err().unwrap().to_string().contains("distinct net budget"));
    }
    #[test]
    fn retained_admission_survives_caller_drop_and_workers_hold_it_through_actual_capture() {
        let _serial = TEST_LOCK.lock().unwrap();
        let engine = EngineHandle::new(Project::new("Retained work"));
        let a = engine.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap();
        let b = engine.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap();
        assert!(engine.prepare_unrouted_nets(UnroutedNetQuery {}).is_err());
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let worker_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            let (lock, wake) = &*worker_gate;
            let mut ready = lock.lock().unwrap();
            while !*ready {
                ready = wake.wait(ready).unwrap();
            }
            drop(ready);
            a.run_value().unwrap()
        });
        drop(worker); // Losing the awaiting handle cannot retire the actual worker's permit.
        assert!(engine.prepare_unrouted_nets(UnroutedNetQuery {}).is_err());
        drop(b);
        let c = engine.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap();
        {
            let (lock, wake) = &*gate;
            *lock.lock().unwrap() = true;
            wake.notify_one();
        }
        drop(c);
        for _ in 0..100 {
            if ACTIVE.load(Ordering::Acquire) == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("retained worker must finish and release admission");
    }
}
