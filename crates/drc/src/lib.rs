//! Read-only, scoped batch checks for SMD pads, straight tracks and through vias.
//! Reports explicit coverage; this is not complete manufacturing DRC.
mod geometry;

use geometry::{EPS, P, Shape, bounds, contains, edges, point, rectangle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{CopperLayer, ObjectId, Project, Side};
use viamind_geom::{Nm, Rect, SpatialIndex};

pub const MAX_PADS: usize = 100_000;
pub const MAX_COPPER_SHAPES: usize = 100_000;
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
    #[schemars(range(min = 0, max = 100))]
    pub minimum_via_drill_mm: f64,
    #[schemars(range(min = 0, max = 100))]
    pub minimum_annular_width_mm: f64,
    #[schemars(range(min = 0, max = 100))]
    pub hole_clearance_mm: f64,
    #[schemars(range(min = 0, max = 100))]
    pub hole_to_copper_clearance_mm: f64,
}
impl Default for DrcInput {
    fn default() -> Self {
        Self {
            copper_clearance_mm: 0.2,
            edge_clearance_mm: 0.25,
            max_violations: 1000,
            minimum_via_drill_mm: 0.2,
            minimum_annular_width_mm: 0.1,
            hole_clearance_mm: 0.25,
            hole_to_copper_clearance_mm: 0.2,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub copper_clearance: Nm,
    pub edge_clearance: Nm,
    pub minimum_via_drill: Nm,
    pub minimum_annular_width: Nm,
    pub hole_clearance: Nm,
    pub hole_to_copper_clearance: Nm,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("DRC: {0}")]
pub struct DrcError(pub String);
impl DrcInput {
    pub fn validate(&self) -> Result<Rules, DrcError> {
        for (name, value) in [
            ("copper_clearance_mm", self.copper_clearance_mm),
            ("edge_clearance_mm", self.edge_clearance_mm),
            ("minimum_via_drill_mm", self.minimum_via_drill_mm),
            ("minimum_annular_width_mm", self.minimum_annular_width_mm),
            ("hole_clearance_mm", self.hole_clearance_mm),
            ("hole_to_copper_clearance_mm", self.hole_to_copper_clearance_mm),
        ] {
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(DrcError(format!("{name} must be finite and within 0..100 mm")));
            }
        }
        if !(1..=10_000).contains(&self.max_violations) {
            return Err(DrcError("max_violations must be within 1..10000".into()));
        }
        Ok(Rules {
            copper_clearance: Nm::from_mm(self.copper_clearance_mm),
            edge_clearance: Nm::from_mm(self.edge_clearance_mm),
            minimum_via_drill: Nm::from_mm(self.minimum_via_drill_mm),
            minimum_annular_width: Nm::from_mm(self.minimum_annular_width_mm),
            hole_clearance: Nm::from_mm(self.hole_clearance_mm),
            hole_to_copper_clearance: Nm::from_mm(self.hole_to_copper_clearance_mm),
        })
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
/// Stable mixed copper references. Pads retain parent-part selection semantics.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopperReference {
    Pad {
        #[serde(flatten)]
        pad: PadReference,
    },
    Track {
        track_id: ObjectId,
        side: Side,
        net: Option<String>,
    },
    Via {
        via_id: ObjectId,
        net: Option<String>,
    },
    NonplatedHole {
        part_id: ObjectId,
        hole_id: ObjectId,
        key: String,
    },
}
impl CopperReference {
    pub fn selection_id(&self) -> ObjectId {
        match self {
            Self::Pad { pad } => pad.part_id,
            Self::NonplatedHole { part_id, .. } => *part_id,
            Self::Track { track_id, .. } => *track_id,
            Self::Via { via_id, .. } => *via_id,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Pad { pad } => {
                format!("{} pad {} ({}) · part {} · {:?} · {:?}", pad.designator, pad.pad_number, pad.pad_id, pad.part_id, pad.side, pad.net)
            }
            Self::Track { track_id, side, net } => format!("track {track_id} · {side:?} · {net:?}"),
            Self::Via { via_id, net } => format!("via {via_id} · top/bottom · {net:?}"),
            Self::NonplatedHole { part_id, hole_id, key } => format!("nonplated hole {key} ({hole_id}) · part {part_id} · top/bottom"),
        }
    }
    fn net(&self) -> &Option<String> {
        match self {
            Self::Pad { pad } => &pad.net,
            Self::NonplatedHole { .. } => &None,
            Self::Track { net, .. } | Self::Via { net, .. } => net,
        }
    }
    fn layers(&self) -> u8 {
        match self {
            Self::Pad { pad } => side_mask(pad.side),
            Self::Track { side, .. } => side_mask(*side),
            Self::Via { .. } | Self::NonplatedHole { .. } => 3,
        }
    }
}
fn side_mask(side: Side) -> u8 {
    if side == Side::Top { 1 } else { 2 }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    CopperClearance,
    MinimumTrackWidth,
    OutsideSubstrate,
    BoardEdgeClearance,
    MinimumViaDrill,
    MinimumAnnularWidth,
    HoleClearance,
    HoleToCopperClearance,
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
    pub objects: Vec<CopperReference>,
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
    pub checked_tracks: usize,
    pub checked_vias: usize,
    pub checked_holes: usize,
    pub unassigned_tracks: usize,
    pub unassigned_vias: usize,
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
    #[serde(
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty",
        deserialize_with = "viamind_doc::net_rules::deserialize_named_net_rules"
    )]
    pub document_net_rules: viamind_doc::NamedNetRules,
    pub violations: Vec<Violation>,
    pub coverage: Coverage,
    pub truncated: bool,
    pub incomplete_reason: Option<String>,
}
struct Copper {
    reference: CopperReference,
    shape: Option<Shape>,
    hole: Option<Shape>,
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
        let shape = copper.shape.or(copper.hole).expect("physical copper or drill");
        // A boundary segment through the open pad interior detects notches and enclosed holes.
        let mut nearby: Vec<_> = self.index.query_rect(&copper.bbox).map(|(i, _)| i).collect();
        nearby.sort_unstable();
        for i in nearby {
            let (a, b, kind) = self.edges[i];
            if shape.segment_enters(a, b) {
                return Some((ViolationKind::OutsideSubstrate, kind, 0.0));
            }
        }
        for (kind, ring, bbox) in &self.rings {
            let outside = match kind {
                Boundary::Outer => !contains(ring, shape.sample(), true),
                Boundary::Cutout { .. } => bbox.intersects(&copper.bbox) && contains(ring, shape.sample(), false),
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
            let distance = shape.segment_distance(a, b);
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
    viamind_doc::named_net_rule_budget(&project.board.net_rules).map_err(|e| DrcError(e.into()))?;
    let rules = input.validate()?;
    viamind_doc::footprint_holes::preflight_project(project, None).map_err(DrcError)?;
    // Charge every physical row before building geometry, including mechanical-only Parts.
    let authored_holes = project.board.parts.values().any(|p| p.holes.is_some());
    let mut physical_rows = project.board.tracks.len().saturating_add(project.board.vias.len());
    let mut npth_count = 0usize;
    for part in project.board.parts.values() {
        let holes = part.holes.as_ref().map_or(0, Vec::len);
        npth_count = npth_count.saturating_add(holes);
        physical_rows = physical_rows.saturating_add(holes).saturating_add(part.pads.as_ref().map_or(0, |p| p.0.len()));
        if authored_holes && physical_rows > MAX_COPPER_SHAPES {
            return Err(DrcError(format!("at most {MAX_COPPER_SHAPES} physical pad/track/via/hole shapes are supported per run")));
        }
    }
    if let Some(outline) = &project.board.outline {
        outline.validate().map_err(|e| DrcError(e.to_string()))?;
    }
    let mut report = DrcReport {
        project_id: project.id,
        document_net_rules: project.board.net_rules.clone(),
        rules,
        violations: Vec::new(),
        truncated: false,
        incomplete_reason: None,
        coverage: Coverage {
            checked_pads: 0,
            unassigned_pads: 0,
            checked_tracks: 0,
            checked_vias: 0,
            checked_holes: 0,
            unassigned_tracks: 0,
            unassigned_vias: 0,
            excluded_parts: Vec::new(),
            excluded_part_count: 0,
            excluded_details_truncated: false,
            explicit_outline_checked: project.board.outline.is_some(),
            examined_candidate_pairs: 0,
            checks_not_performed: [
                "curved tracks, blind/buried vias and inner copper layers",
                "copper pours",
                "through-hole footprint pads, nonplated holes and drill-to-edge clearance",
                "solder mask and silkscreen",
                "schematic/net connectivity and unrouted connections",
                "manufacturing constraints",
            ]
            .map(str::to_owned)
            .to_vec(),
        },
    };
    if authored_holes {
        report.coverage.checks_not_performed[2] = "through-hole footprint pads and plated drill-to-edge clearance".into();
    }
    if project.board.outline.is_none() {
        report.coverage.checks_not_performed.push("board-edge/substrate checks: no explicit board outline".into());
    }
    let mut copper = Vec::new();
    for part in project.board.parts.values() {
        if let Some(holes) = &part.holes {
            viamind_doc::footprint_holes::validate_part_holes(holes).map_err(DrcError)?;
            for hole in holes {
                let resolved = viamind_doc::footprint_holes::resolve_hole(part, hole).map_err(DrcError)?;
                let center = point(resolved.center);
                let shape = Shape::Capsule { a: center, b: center, radius: resolved.diameter.0 as f64 / 2.0 };
                copper.push(Copper {
                    reference: CopperReference::NonplatedHole { part_id: part.id, hole_id: hole.id, key: hole.key.clone() },
                    shape: None,
                    hole: Some(shape),
                    bbox: shape.bbox(),
                });
            }
        }
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
        if report.coverage.checked_pads + pads.0.len() > MAX_PADS {
            return Err(DrcError(format!("at most {MAX_PADS} explicit pads are supported per run")));
        }
        for pad in &pads.0 {
            let shape = Shape::Rectangle(rectangle(part, pad));
            let bbox = shape.bbox();
            let reference = CopperReference::Pad {
                pad: PadReference {
                    part_id: part.id,
                    pad_id: pad.id,
                    designator: part.designator.clone(),
                    pad_number: pad.number.clone(),
                    side: part.side,
                    net: pad.net.clone(),
                },
            };
            report.coverage.checked_pads += 1;
            report.coverage.unassigned_pads += usize::from(pad.net.is_none());
            copper.push(Copper { reference, shape: Some(shape), hole: None, bbox });
        }
    }
    report.coverage.checked_holes = npth_count;
    if copper.len().saturating_add(project.board.tracks.len()).saturating_add(project.board.vias.len()) > MAX_COPPER_SHAPES {
        return Err(DrcError(format!("at most {MAX_COPPER_SHAPES} total pad/track/via copper shapes are supported per run")));
    }
    for track in project.board.tracks.values() {
        track.validate().map_err(|e| DrcError(format!("track {}: {e}", track.id)))?;
        let shape = Shape::Capsule { a: point(track.start), b: point(track.end), radius: track.width.0 as f64 / 2.0 };
        let side = match track.layer {
            CopperLayer::Top => Side::Top,
            CopperLayer::Bottom => Side::Bottom,
        };
        copper.push(Copper {
            reference: CopperReference::Track { track_id: track.id, side, net: track.net.clone() },
            shape: Some(shape),
            hole: None,
            bbox: shape.bbox(),
        });
        let minimum = viamind_doc::named_track_width(&report.document_net_rules, track.net.as_deref());
        if track.width < minimum && !report.truncated {
            report.add(
                Violation {
                    kind: ViolationKind::MinimumTrackWidth,
                    objects: vec![CopperReference::Track { track_id: track.id, side, net: track.net.clone() }],
                    boundary: None,
                    measured_clearance: track.width,
                    required_clearance: minimum,
                    message: "track width is below the exact named-net document minimum".into(),
                },
                input.max_violations,
            );
        }
        report.coverage.checked_tracks += 1;
        report.coverage.unassigned_tracks += usize::from(track.net.is_none());
    }
    for via in project.board.vias.values() {
        via.validate().map_err(|e| DrcError(format!("via {}: {e}", via.id)))?;
        let center = point(via.position);
        let shape = Shape::Annulus { center, inner: via.drill.0 as f64 / 2.0, outer: via.diameter.0 as f64 / 2.0 };
        let reference = CopperReference::Via { via_id: via.id, net: via.net.clone() };
        copper.push(Copper {
            reference: reference.clone(),
            shape: Some(shape),
            hole: Some(Shape::Capsule { a: center, b: center, radius: via.drill.0 as f64 / 2.0 }),
            bbox: shape.bbox(),
        });
        report.coverage.checked_vias += 1;
        report.coverage.checked_holes += 1;
        report.coverage.unassigned_vias += usize::from(via.net.is_none());
        let drill_minimum = viamind_doc::net_rules::named_via_drill(&project.board.net_rules, report.rules.minimum_via_drill, via.net.as_deref());
        let annular_minimum =
            viamind_doc::net_rules::named_annular_width(&project.board.net_rules, report.rules.minimum_annular_width, via.net.as_deref());
        let doubled_annulus = via.diameter.0.checked_sub(via.drill.0).ok_or_else(|| DrcError("via annulus overflow".into()))?;
        let doubled_required = annular_minimum.0.checked_mul(2).ok_or_else(|| DrcError("via annular minimum overflow".into()))?;
        for (kind, measured, required, below, message) in [
            (ViolationKind::MinimumViaDrill, via.drill.0 as f64, drill_minimum, via.drill < drill_minimum, "via drill diameter is below minimum"),
            (
                ViolationKind::MinimumAnnularWidth,
                doubled_annulus as f64 / 2.0,
                annular_minimum,
                doubled_annulus < doubled_required,
                "via radial annular width is below minimum",
            ),
        ] {
            if below && !report.truncated {
                report.add(
                    Violation {
                        kind,
                        objects: vec![reference.clone()],
                        boundary: None,
                        measured_clearance: Nm(measured.floor() as i64),
                        required_clearance: required,
                        message: message.into(),
                    },
                    input.max_violations,
                );
            }
        }
    }
    copper.sort_by(|a, b| a.reference.cmp(&b.reference));
    let substrate = project.board.outline.as_ref().map(Substrate::new);
    if let Some(substrate) = substrate {
        for pad in &copper {
            if report.truncated {
                break;
            }
            if let Some((kind, boundary, distance)) = substrate.violation(pad, report.rules.edge_clearance) {
                let message = if matches!(pad.reference, CopperReference::NonplatedHole { .. }) {
                    if kind == ViolationKind::OutsideSubstrate {
                        "nonplated drill extends outside the board or into a cutout"
                    } else {
                        "nonplated drill is too close to the board/cutout boundary"
                    }
                } else if kind == ViolationKind::OutsideSubstrate {
                    "copper extends outside the board or into a cutout"
                } else {
                    "copper is too close to the board/cutout boundary"
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
    let search_clearance = viamind_doc::maximum_named_clearance(&report.document_net_rules, report.rules.copper_clearance)
        .max(report.rules.hole_clearance)
        .max(report.rules.hole_to_copper_clearance);
    'objects: for (i, pad) in copper.iter().enumerate() {
        if report.truncated {
            break;
        }
        let mut candidates: Vec<_> = index.query_rect(&pad.bbox.inflate(search_clearance)).map(|(j, _)| j).filter(|j| *j > i).collect();
        candidates.sort_unstable();
        for j in candidates {
            let other = &copper[j];
            if report.coverage.examined_candidate_pairs == MAX_CANDIDATE_PAIRS {
                report.truncated = true;
                report.incomplete_reason = Some(format!("candidate-pair limit {MAX_CANDIDATE_PAIRS} reached; copper/hole checks incomplete"));
                break 'objects;
            }
            report.coverage.examined_candidate_pairs += 1;
            let same_net = pad.reference.net().is_some() && pad.reference.net() == other.reference.net();
            let shared_layer = pad.reference.layers() & other.reference.layers() != 0;
            let mut checks = Vec::with_capacity(4);
            if let (Some(a), Some(b)) = (pad.shape, other.shape)
                && shared_layer
                && !same_net
            {
                checks.push((
                    ViolationKind::CopperClearance,
                    a.distance(b),
                    viamind_doc::named_copper_clearance(
                        &report.document_net_rules,
                        report.rules.copper_clearance,
                        pad.reference.net().as_deref(),
                        other.reference.net().as_deref(),
                    ),
                    "different-net or unassigned copper touches/overlaps or is below clearance",
                ));
            }
            // NPTH is a physical exclusion on both sides, for every net including its owner's copper.
            // Existing plated-via ownership exemption remains limited to that plated drill.
            for (drill, other) in [(pad, other), (other, pad)] {
                if shared_layer
                    && (drill.shape.is_none() || !same_net)
                    && let (Some(hole), Some(shape)) = (drill.hole, other.shape)
                {
                    checks.push((
                        ViolationKind::HoleToCopperClearance,
                        hole.distance(shape),
                        report.rules.hole_to_copper_clearance,
                        if drill.shape.is_none() {
                            "nonplated drill touches/overlaps copper or is below hole-to-copper clearance"
                        } else {
                            "via drill touches/overlaps foreign copper or is below hole-to-copper clearance"
                        },
                    ));
                }
            }
            if let (Some(a), Some(b)) = (pad.hole, other.hole) {
                checks.push((
                    ViolationKind::HoleClearance,
                    a.distance(b),
                    report.rules.hole_clearance,
                    if pad.shape.is_none() || other.shape.is_none() {
                        "physical drilled holes touch/overlap or are below physical hole spacing"
                    } else {
                        "drilled via holes touch/overlap or are below physical hole spacing"
                    },
                ));
            }
            // Both directed hole/copper checks may have the same kind; retain the smallest measured gap once.
            checks.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
            checks.dedup_by_key(|check| check.0);
            for (kind, distance, required, message) in checks {
                if distance <= EPS || distance + EPS < required.0 as f64 {
                    report.add(
                        Violation {
                            kind,
                            objects: vec![pad.reference.clone(), other.reference.clone()],
                            boundary: None,
                            measured_clearance: Nm(distance.floor() as i64),
                            required_clearance: required,
                            message: message.into(),
                        },
                        input.max_violations,
                    );
                    if report.truncated {
                        break 'objects;
                    }
                }
            }
        }
    }
    report.violations.sort();
    Ok(report)
}
