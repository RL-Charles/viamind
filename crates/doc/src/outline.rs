//! Validated polygon substrate geometry, stored on the integer nanometre grid.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::{Polygon, SegmentIntersection};

pub const MAX_OUTLINE_VERTICES: usize = 2048;
pub const MAX_OUTLINE_CUTOUTS: usize = 64;
pub const MAX_OUTLINE_COORD_NM: i64 = 10_000_000_000;

/// One simple outer boundary with strictly internal, disjoint cutouts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "OutlineInput")]
pub struct BoardOutline {
    pub outer: Polygon,
    pub cutouts: Vec<Polygon>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OutlineInput {
    outer: Polygon,
    #[serde(default)]
    cutouts: Vec<Polygon>,
}
impl TryFrom<OutlineInput> for BoardOutline {
    type Error = OutlineError;
    fn try_from(input: OutlineInput) -> Result<Self, Self::Error> {
        Self::new(input.outer, input.cutouts)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid board outline: {0}")]
pub struct OutlineError(pub String);

impl BoardOutline {
    pub fn new(outer: Polygon, cutouts: Vec<Polygon>) -> Result<Self, OutlineError> {
        let outline = Self { outer, cutouts };
        outline.validate()?;
        Ok(outline)
    }
    pub fn rings(&self) -> impl Iterator<Item = &Polygon> {
        std::iter::once(&self.outer).chain(&self.cutouts)
    }
    pub fn validate(&self) -> Result<(), OutlineError> {
        let invalid = |message: &str| OutlineError(message.into());
        if self.cutouts.len() > MAX_OUTLINE_CUTOUTS || self.rings().map(|p| p.points.len()).sum::<usize>() > MAX_OUTLINE_VERTICES {
            return Err(invalid("at most 2048 total vertices and 64 cutouts are supported"));
        }
        for (index, ring) in self.rings().enumerate() {
            validate_ring(ring).map_err(|e| OutlineError(format!("ring {index}: {}", e.0)))?;
        }
        for (i, hole) in self.cutouts.iter().enumerate() {
            if crosses(&self.outer, hole) || !self.outer.contains(hole.points[0]) {
                return Err(invalid("cutouts must be strictly inside the outer boundary without touching it"));
            }
            for other in &self.cutouts[..i] {
                if crosses(other, hole) || other.contains(hole.points[0]) || hole.contains(other.points[0]) {
                    return Err(invalid("cutouts cannot touch, overlap or contain another cutout"));
                }
            }
        }
        Ok(())
    }
}
fn validate_ring(ring: &Polygon) -> Result<(), OutlineError> {
    let invalid = |message: &str| OutlineError(message.into());
    if ring.points.len() < 3 {
        return Err(invalid("at least three vertices are required"));
    }
    if ring.points.iter().any(|p| p.x.0.unsigned_abs() > MAX_OUTLINE_COORD_NM as u64 || p.y.0.unsigned_abs() > MAX_OUTLINE_COORD_NM as u64) {
        return Err(invalid("coordinates must be within +/-10000 mm"));
    }
    let edges: Vec<_> = ring.edges().collect();
    if edges.iter().any(|e| e.a == e.b) {
        return Err(invalid("adjacent vertices must differ; do not repeat the first vertex at the end"));
    }
    if ring.signed_area_x2() == 0 {
        return Err(invalid("ring must have nonzero area"));
    }
    for (i, edge) in edges.iter().enumerate() {
        for (j, other) in edges.iter().enumerate().skip(i + 1) {
            let adjacent = j == i + 1 || (i == 0 && j == edges.len() - 1);
            match edge.intersection(other) {
                SegmentIntersection::None => (),
                SegmentIntersection::Point(p) if adjacent && (p == edge.a || p == edge.b) && (p == other.a || p == other.b) => (),
                _ => return Err(invalid("edges cannot cross, touch nonadjacent edges or overlap")),
            }
        }
    }
    Ok(())
}
fn crosses(a: &Polygon, b: &Polygon) -> bool {
    a.edges().any(|a| b.edges().any(|b| !matches!(a.intersection(&b), SegmentIntersection::None)))
}
