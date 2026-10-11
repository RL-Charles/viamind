//! Exact user-selected path postures; no obstacle search or electrical inference.
use crate::snap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::{Nm, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CornerMode {
    Cardinal,
    Octilinear,
}
/// Primary means horizontal-first (cardinal) or straight-first (octilinear).
/// Alternate means vertical-first (cardinal) or diagonal-first (octilinear).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Posture {
    #[default]
    Primary,
    Alternate,
}
impl Posture {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Primary => Self::Alternate,
            Self::Alternate => Self::Primary,
        };
    }
    pub fn label(self, mode: CornerMode) -> &'static str {
        match (mode, self) {
            (CornerMode::Cardinal, Self::Primary) => "Horizontal first",
            (CornerMode::Cardinal, Self::Alternate) => "Vertical first",
            (CornerMode::Octilinear, Self::Primary) => "Straight first",
            (CornerMode::Octilinear, Self::Alternate) => "Diagonal first",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PadPathPlan {
    pub points: Vec<Point>,
    /// Short unconstrained-angle segments between exact pads and nearest grid anchors.
    pub endpoint_stub_segments: usize,
}
fn in_range(p: Point) -> bool {
    p.x.0.unsigned_abs() <= 10_000_000_000 && p.y.0.unsigned_abs() <= 10_000_000_000
}
fn push(points: &mut Vec<Point>, point: Point) -> Result<(), &'static str> {
    if points.last() != Some(&point) {
        if points.len() >= 1025 {
            return Err("planned path exceeds 1024 segments; remove waypoints");
        }
        points.push(point);
    }
    Ok(())
}
/// Preserve exact pad centers with explicit nearest-grid stubs, then plan every
/// grid-anchor pair with one selected cardinal/45-degree posture. No model mutation.
pub fn plan_pad_path(
    start: Point,
    target: Point,
    waypoints: &[Point],
    grid: Nm,
    mode: CornerMode,
    posture: Posture,
) -> Result<PadPathPlan, &'static str> {
    if grid.0 <= 0 || grid.0 > 10_000_000_000 {
        return Err("grid must be positive and at most 10000 mm");
    }
    if waypoints.len() > 510 {
        return Err("at most 510 confirmed waypoints are supported");
    }
    if !in_range(start) || !in_range(target) || waypoints.iter().any(|p| !in_range(*p)) {
        return Err("route coordinates must stay within ±10000 mm");
    }
    for point in waypoints {
        if snap(*point, grid)? != *point {
            return Err("supplied waypoints must already lie exactly on the grid");
        }
    }
    let first = snap(start, grid)?;
    let last = snap(target, grid)?;
    if !in_range(first) || !in_range(last) {
        return Err("nearest pad grid anchor exceeds ±10000 mm; choose a smaller grid");
    }
    let mut points = Vec::with_capacity((waypoints.len() + 1) * 2 + 3);
    push(&mut points, start)?;
    push(&mut points, first)?;
    let mut previous = first;
    for next in waypoints.iter().copied().chain([last]) {
        // Bounds above make subtraction safe; i128 keeps arithmetic explicit.
        let dx = i128::from(next.x.0) - i128::from(previous.x.0);
        let dy = i128::from(next.y.0) - i128::from(previous.y.0);
        let (x, y) = match mode {
            CornerMode::Cardinal => match posture {
                Posture::Primary => (next.x.0, previous.y.0),
                Posture::Alternate => (previous.x.0, next.y.0),
            },
            CornerMode::Octilinear => {
                let diagonal = dx.abs().min(dy.abs());
                let diagonal_x = diagonal * dx.signum();
                let diagonal_y = diagonal * dy.signum();
                let (x, y) = match posture {
                    Posture::Primary => (i128::from(next.x.0) - diagonal_x, i128::from(next.y.0) - diagonal_y),
                    Posture::Alternate => (i128::from(previous.x.0) + diagonal_x, i128::from(previous.y.0) + diagonal_y),
                };
                (i64::try_from(x).map_err(|_| "planned bend overflow")?, i64::try_from(y).map_err(|_| "planned bend overflow")?)
            }
        };
        push(&mut points, Point::new(Nm(x), Nm(y)))?;
        push(&mut points, next)?;
        previous = next;
    }
    push(&mut points, target)?;
    if points.len() < 2 {
        return Err("planned route has no nonzero segments");
    }
    Ok(PadPathPlan { points, endpoint_stub_segments: usize::from(start != first) + usize::from(target != last) })
}
