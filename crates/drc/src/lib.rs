//! Read-only, scoped batch checks for persisted rectangular SMD pads.
//! Reports explicit coverage; this is not complete manufacturing DRC.
mod geometry;

use geometry::{EPS, P, bounds, contains, edges, enters_rectangle, point, rectangle, rectangle_distance, segment_distance};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project, Side};
use viamind_geom::{Nm, Rect, SpatialIndex};

pub const MAX_PADS: usize = 100_000;
pub const MAX_CANDIDATE_PAIRS: usize = 1_000_000;
pub const MAX_EXCLUDED_DETAILS: usize = 1000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DrcInput {
    #[schemars(range(min = 0, max = 100))]
    pub copper_clearance_mm: f64,
    #[schemars(range(min = 0, max = 100))]
    pub edge_clearance_mm: f64,
    #[schemars(range(min = 1, max = 10000))]
    pub max_violations: usize,
}
impl Default for DrcInput {
    fn default() -> Self {
        Self { copper_clearance_mm: 0.2, edge_clearance_mm: 0.25, max_violations: 1000 }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub copper_clearance: Nm,
    pub edge_clearance: Nm,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("DRC: {0}")]
pub struct DrcError(pub String);
impl DrcInput {
    pub fn validate(&self) -> Result<Rules, DrcError> {
        for (name, value) in [("copper_clearance_mm", self.copper_clearance_mm), ("edge_clearance_mm", self.edge_clearance_mm)] {
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(DrcError(format!("{name} must be finite and within 0..100 mm")));
            }
        }
        if !(1..=10_000).contains(&self.max_violations) {
            return Err(DrcError("max_violations must be within 1..10000".into()));
        }
        Ok(Rules { copper_clearance: Nm::from_mm(self.copper_clearance_mm), edge_clearance: Nm::from_mm(self.edge_clearance_mm) })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PadReference {
    pub part_id: ObjectId,
    pub pad_id: ObjectId,
    pub designator: String,
    pub pad_number: String,
    pub side: Side,
    pub net: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    CopperClearance,
    OutsideSubstrate,
    BoardEdgeClearance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Boundary {
    Outer,
    Cutout { index: usize },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Violation {
    pub kind: ViolationKind,
    pub objects: Vec<PadReference>,
    pub boundary: Option<Boundary>,
    pub measured_clearance: Nm,
    pub required_clearance: Nm,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedPart {
    pub part_id: ObjectId,
    pub designator: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub checked_pads: usize,
    pub unassigned_pads: usize,
    pub excluded_parts: Vec<ExcludedPart>,
    pub excluded_part_count: usize,
    pub excluded_details_truncated: bool,
    pub explicit_outline_checked: bool,
    pub examined_candidate_pairs: usize,
    pub checks_not_performed: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrcReport {
    pub project_id: ObjectId,
    pub rules: Rules,
    pub violations: Vec<Violation>,
    pub coverage: Coverage,
    pub truncated: bool,
    pub incomplete_reason: Option<String>,
}
struct Copper {
    reference: PadReference,
    corners: [P; 4],
    bbox: Rect,
}
struct Substrate {
    rings: Vec<(Boundary, Vec<P>, Rect)>,
    edges: Vec<(P, P, Boundary)>,
    index: SpatialIndex<usize>,
}
impl Substrate {
    fn new(outline: &viamind_doc::BoardOutline) -> Self {
        let rings: Vec<_> = outline
            .rings()
            .enumerate()
            .map(|(i, ring)| {
                let kind = if i == 0 { Boundary::Outer } else { Boundary::Cutout { index: i - 1 } };
                let points: Vec<_> = ring.points.iter().copied().map(point).collect();
                let bbox = bounds(&points);
                (kind, points, bbox)
            })
            .collect();
        let segments: Vec<_> = rings.iter().flat_map(|(kind, points, _)| edges(points).map(|(a, b)| (a, b, *kind))).collect();
        let index = SpatialIndex::bulk_load(segments.iter().enumerate().map(|(i, (a, b, _))| (i, bounds(&[*a, *b]))));
        Self { rings, edges: segments, index }
    }
    fn violation(&self, copper: &Copper, required: Nm) -> Option<(ViolationKind, Boundary, f64)> {
        // A boundary segment through the open pad interior detects notches and enclosed holes.
        let mut nearby: Vec<_> = self.index.query_rect(&copper.bbox).map(|(i, _)| i).collect();
        nearby.sort_unstable();
        for i in nearby {
            let (a, b, kind) = self.edges[i];
            if enters_rectangle(&copper.corners, a, b) {
                return Some((ViolationKind::OutsideSubstrate, kind, 0.0));
            }
        }
        for (kind, ring, bbox) in &self.rings {
            let outside = match kind {
                Boundary::Outer => copper.corners.iter().any(|p| !contains(ring, *p, true)),
                Boundary::Cutout { .. } => bbox.intersects(&copper.bbox) && copper.corners.iter().any(|p| contains(ring, *p, false)),
            };
            if outside {
                return Some((ViolationKind::OutsideSubstrate, *kind, 0.0));
            }
        }
        let mut nearest = None;
        let mut candidates: Vec<_> = self.index.query_rect(&copper.bbox.inflate(required)).map(|(i, _)| i).collect();
        candidates.sort_unstable();
        for i in candidates {
            let (a, b, kind) = self.edges[i];
            let distance = edges(&copper.corners).map(|(c, d)| segment_distance(a, b, c, d)).fold(f64::INFINITY, f64::min);
            if distance + EPS < required.0 as f64 && nearest.as_ref().is_none_or(|(_, _, best)| distance < *best) {
                nearest = Some((ViolationKind::BoardEdgeClearance, kind, distance));
            }
        }
        nearest
    }
}
impl DrcReport {
    fn add(&mut self, issue: Violation, limit: usize) {
        if self.violations.len() == limit {
            self.truncated = true;
            self.incomplete_reason = Some(format!("result limit {limit} reached; more violations exist"));
        } else {
            self.violations.push(issue);
        }
    }
}
/// Evaluate a captured immutable project. No model, selection, history or file state changes.
pub fn run_drc(project: &Project, input: &DrcInput) -> Result<DrcReport, DrcError> {
    let rules = input.validate()?;
    if let Some(outline) = &project.board.outline {
        outline.validate().map_err(|e| DrcError(e.to_string()))?;
    }
    let mut report = DrcReport {
        project_id: project.id,
        rules,
        violations: Vec::new(),
        truncated: false,
        incomplete_reason: None,
        coverage: Coverage {
            checked_pads: 0,
            unassigned_pads: 0,
            excluded_parts: Vec::new(),
            excluded_part_count: 0,
            excluded_details_truncated: false,
            explicit_outline_checked: project.board.outline.is_some(),
            examined_candidate_pairs: 0,
            checks_not_performed: [
                "tracks and vias",
                "copper pours",
                "drills and annular rings",
                "solder mask and silkscreen",
                "schematic/net connectivity and unrouted connections",
                "manufacturing constraints",
            ]
            .map(str::to_owned)
            .to_vec(),
        },
    };
    if project.board.outline.is_none() {
        report.coverage.checks_not_performed.push("board-edge/substrate checks: no explicit board outline".into());
    }
    let mut copper = Vec::new();
    for part in project.board.parts.values() {
        let reason = if part.pads.is_none() {
            Some("legacy footprint preview has no explicit copper-pad model")
        } else if [part.position.x.0, part.position.y.0].iter().any(|n| n.unsigned_abs() > 10_000_000_000)
            || !(0..360_000).contains(&part.rotation_mdeg)
        {
            Some("unsupported transform: origin must be within +/-10000 mm and rotation within 0..360000 millidegrees")
        } else {
            None
        };
        if let Some(reason) = reason {
            report.coverage.excluded_part_count += 1;
            if report.coverage.excluded_parts.len() < MAX_EXCLUDED_DETAILS {
                report.coverage.excluded_parts.push(ExcludedPart { part_id: part.id, designator: part.designator.clone(), reason: reason.into() });
            } else {
                report.coverage.excluded_details_truncated = true;
            }
            continue;
        }
        let pads = part.pads.as_ref().expect("modeled part");
        viamind_doc::Pads::try_from(pads.0.clone()).map_err(|e| DrcError(format!("part {}: {e}", part.id)))?;
        if copper.len() + pads.0.len() > MAX_PADS {
            return Err(DrcError(format!("at most {MAX_PADS} explicit pads are supported per run")));
        }
        for pad in &pads.0 {
            let corners = rectangle(part, pad);
            let bbox = bounds(&corners);
            let reference = PadReference {
                part_id: part.id,
                pad_id: pad.id,
                designator: part.designator.clone(),
                pad_number: pad.number.clone(),
                side: part.side,
                net: pad.net.clone(),
            };
            copper.push(Copper { reference, corners, bbox });
        }
    }
    copper.sort_by(|a, b| a.reference.cmp(&b.reference));
    report.coverage.checked_pads = copper.len();
    report.coverage.unassigned_pads = copper.iter().filter(|p| p.reference.net.is_none()).count();
    let substrate = project.board.outline.as_ref().map(Substrate::new);
    if let Some(substrate) = substrate {
        for pad in &copper {
            if let Some((kind, boundary, distance)) = substrate.violation(pad, report.rules.edge_clearance) {
                let message = if kind == ViolationKind::OutsideSubstrate {
                    "pad copper extends outside the board or into a cutout"
                } else {
                    "pad copper is too close to the board/cutout boundary"
                };
                report.add(
                    Violation {
                        kind,
                        objects: vec![pad.reference.clone()],
                        boundary: Some(boundary),
                        measured_clearance: Nm(distance.floor() as i64),
                        required_clearance: report.rules.edge_clearance,
                        message: message.into(),
                    },
                    input.max_violations,
                );
                if report.truncated {
                    break;
                }
            }
        }
    }
    let index = SpatialIndex::bulk_load(copper.iter().enumerate().map(|(i, pad)| (i, pad.bbox)));
    'pads: for (i, pad) in copper.iter().enumerate() {
        if report.truncated {
            break;
        }
        let mut candidates: Vec<_> = index.query_rect(&pad.bbox.inflate(report.rules.copper_clearance)).map(|(j, _)| j).filter(|j| *j > i).collect();
        candidates.sort_unstable();
        for j in candidates {
            let other = &copper[j];
            if report.coverage.examined_candidate_pairs == MAX_CANDIDATE_PAIRS {
                report.truncated = true;
                report.incomplete_reason = Some(format!("candidate-pair limit {MAX_CANDIDATE_PAIRS} reached; copper checks incomplete"));
                break 'pads;
            }
            report.coverage.examined_candidate_pairs += 1;
            if pad.reference.side != other.reference.side || (pad.reference.net.is_some() && pad.reference.net == other.reference.net) {
                continue;
            }
            let distance = rectangle_distance(&pad.corners, &other.corners);
            if distance <= EPS || distance + EPS < report.rules.copper_clearance.0 as f64 {
                report.add(
                    Violation {
                        kind: ViolationKind::CopperClearance,
                        objects: vec![pad.reference.clone(), other.reference.clone()],
                        boundary: None,
                        measured_clearance: Nm(distance.floor() as i64),
                        required_clearance: report.rules.copper_clearance,
                        message: "different-net or unassigned pad copper touches/overlaps or is below clearance".into(),
                    },
                    input.max_violations,
                );
                if report.truncated {
                    break 'pads;
                }
            }
        }
    }
    report.violations.sort();
    Ok(report)
}
