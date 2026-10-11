//! Conservative exact endpoint proofs. Extra guides remain outside this subset.
use std::collections::BTreeMap;

use serde::Serialize;
use viamind_doc::{CopperLayer, Project, Side};
use viamind_geom::Point;

use crate::PadEndpointId;

const MAX_OBJECTS: usize = 100_000;
const MAX_TEXT: usize = 2 * 1024 * 1024;
const MAX_STRING: usize = 4096;
const MAX_NM: u64 = 10_000_000_000;

/// Whole-board proof coverage, independent of query pagination. Never a DRC certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CopperConnectivityCoverage {
    pub mode: &'static str,
    pub budget_fallback: bool,
    pub supported_named_pads: usize,
    pub unsupported_named_pads: usize,
    pub analyzed_tracks: usize,
    pub analyzed_vias: usize,
    pub unsupported_named_copper: usize,
}
impl Default for CopperConnectivityCoverage {
    fn default() -> Self {
        Self {
            mode: "conservative_exact_endpoints",
            budget_fallback: false,
            supported_named_pads: 0,
            unsupported_named_pads: 0,
            analyzed_tracks: 0,
            analyzed_vias: 0,
            unsupported_named_copper: 0,
        }
    }
}
#[derive(Default)]
pub(crate) struct Forest(Vec<usize>);
impl Forest {
    pub fn with_size(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn push(&mut self) -> usize {
        let i = self.0.len();
        self.0.push(i);
        i
    }
    pub fn root(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }
    pub fn join(&mut self, a: usize, b: usize) -> bool {
        let a = self.root(a);
        let b = self.root(b);
        if a == b {
            return false;
        }
        self.0[a.max(b)] = a.min(b);
        true
    }
}
pub(crate) struct CopperProof {
    pub coverage: CopperConnectivityCoverage,
    pub pads: BTreeMap<PadEndpointId, usize>,
}
fn budget(project: &Project) -> bool {
    let board = &project.board;
    let mut count = board.parts.len().saturating_add(board.tracks.len()).saturating_add(board.vias.len());
    if count > MAX_OBJECTS {
        return false;
    }
    let mut text = 0usize;
    let mut string = |value: &str| {
        text = text.saturating_add(value.len());
        value.len() <= MAX_STRING && text <= MAX_TEXT
    };
    for part in board.parts.values() {
        if !string(&part.designator) {
            return false;
        }
        if let Some(pads) = &part.pads {
            count = count.saturating_add(pads.0.len());
            if count > MAX_OBJECTS {
                return false;
            }
            for pad in &pads.0 {
                if !string(&pad.number) || !string(pad.net.as_deref().unwrap_or_default()) {
                    return false;
                }
            }
        }
    }
    for net in board.tracks.values().map(|t| &t.net).chain(board.vias.values().map(|v| &v.net)) {
        if !string(net.as_deref().unwrap_or_default()) {
            return false;
        }
    }
    true
}
fn side(layer: CopperLayer) -> bool {
    layer == CopperLayer::Bottom
}
fn key(net: &str, bottom: bool, point: Point) -> (&str, bool, i64, i64) {
    (net, bottom, point.x.0, point.y.0)
}
fn distance2(a: Point, b: Point) -> i128 {
    let dx = i128::from(a.x.0) - i128::from(b.x.0);
    let dy = i128::from(a.y.0) - i128::from(b.y.0);
    dx * dx + dy * dy
}
impl CopperProof {
    pub fn analyze(project: &Project) -> Self {
        let mut result = Self { coverage: CopperConnectivityCoverage::default(), pads: BTreeMap::new() };
        if !budget(project) {
            result.coverage.budget_fallback = true;
            return result;
        }
        // Borrow all net strings. The budget runs before graph allocation.
        let mut nodes = BTreeMap::new();
        let mut forest = Forest::default();
        let mut spans = Vec::new();
        for track in project.board.tracks.values() {
            let Some(net) = track.net.as_deref() else { continue };
            if track.validate().is_err() {
                result.coverage.unsupported_named_copper += 1;
                continue;
            }
            let span = distance2(track.start, track.end);
            let ends = [track.start, track.end].map(|point| {
                let i = *nodes.entry(key(net, side(track.layer), point)).or_insert_with(|| {
                    spans.push(0);
                    forest.push()
                });
                spans[i] = spans[i].max(span);
                i
            });
            forest.join(ends[0], ends[1]);
            result.coverage.analyzed_tracks += 1;
        }
        for via in project.board.vias.values() {
            let Some(net) = via.net.as_deref() else { continue };
            if via.validate().is_err() {
                result.coverage.unsupported_named_copper += 1;
                continue;
            }
            result.coverage.analyzed_vias += 1;
            let top = nodes.get(&key(net, false, via.position));
            let bottom = nodes.get(&key(net, true, via.position));
            if let (Some(&a), Some(&b)) = (top, bottom) {
                let drill2 = i128::from(via.drill.0).pow(2);
                if 4 * spans[a] > drill2 && 4 * spans[b] > drill2 {
                    forest.join(a, b);
                }
            }
        }
        for part in project.board.parts.values() {
            let Some(pads) = &part.pads else { continue };
            for pad in &pads.0 {
                let Some(net) = pad.net.as_deref() else { continue };
                let center = part.local_to_world_nm_exact(pad.position).ok().filter(|p| {
                    [p.x.0, p.y.0].iter().all(|v| v.unsigned_abs() <= MAX_NM)
                        && [pad.width.0, pad.height.0].iter().all(|v| *v > 0 && v.unsigned_abs() <= MAX_NM)
                        && !net.is_empty()
                });
                let Some(center) = center else {
                    result.coverage.unsupported_named_pads += 1;
                    continue;
                };
                result.coverage.supported_named_pads += 1;
                if let Some(&i) = nodes.get(&key(net, part.side == Side::Bottom, center)) {
                    result.pads.insert(PadEndpointId { part_id: part.id, pad_id: pad.id }, forest.root(i));
                }
            }
        }
        result
    }
}
