//! Simple polygons (a single closed ring of points).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::point::{Point, Vector};
use crate::rect::Rect;
use crate::segment::{Segment, orient};

/// A closed polygon: one ring of vertices, either winding, implicitly closed
/// (the last vertex joins the first; do not repeat it).
///
/// Holes and multi-part regions (copper pours) belong to the boolean layer on
/// top of this type. Point containment uses the non-zero winding rule and
/// counts the boundary as inside; it is exact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Polygon {
    /// The vertices in order.
    pub points: Vec<Point>,
}

impl Polygon {
    /// Creates a polygon from its vertices.
    pub fn new(points: Vec<Point>) -> Polygon {
        Polygon { points }
    }

    /// The rectangle as a counter-clockwise polygon.
    pub fn from_rect(r: &Rect) -> Polygon {
        Polygon::new(r.corners().to_vec())
    }

    /// The edges in order, including the closing edge from the last vertex to the first.
    pub fn edges(&self) -> impl Iterator<Item = Segment> + '_ {
        ring_edges(&self.points)
    }

    /// Twice the signed area in nm²: positive when counter-clockwise. Exact.
    pub fn signed_area_x2(&self) -> i128 {
        let Some(&o) = self.points.first() else { return 0 };
        self.points.windows(2).map(|w| orient(o, w[0], w[1])).sum()
    }

    /// Unsigned area in nm².
    pub fn area(&self) -> f64 {
        self.signed_area_x2().unsigned_abs() as f64 / 2.0
    }

    /// Whether the vertices run counter-clockwise (positive area).
    pub fn is_ccw(&self) -> bool {
        self.signed_area_x2() > 0
    }

    /// Bounding box of the vertices, or `None` for an empty polygon.
    pub fn bbox(&self) -> Option<Rect> {
        Rect::from_points(self.points.iter().copied())
    }

    /// Whether `p` is inside or on the boundary (non-zero winding). Exact.
    pub fn contains(&self, p: Point) -> bool {
        ring_contains(&self.points, p)
    }

    /// Distance from `p` to the polygon, in nanometres; zero inside or on the boundary.
    pub fn distance_to_point(&self, p: Point) -> f64 {
        if self.contains(p) { 0.0 } else { self.edges().map(|e| e.distance_to_point(p)).fold(f64::INFINITY, f64::min) }
    }

    /// The polygon moved by `v`.
    pub fn translate(&self, v: Vector) -> Polygon {
        Polygon::new(self.points.iter().map(|&p| p + v).collect())
    }
}

/// Edges of a closed ring.
pub(crate) fn ring_edges(points: &[Point]) -> impl Iterator<Item = Segment> + '_ {
    let n = points.len();
    (0..n).map(move |i| Segment::new(points[i], points[(i + 1) % n]))
}

/// Non-zero winding containment with the boundary counted as inside.
pub(crate) fn ring_contains(points: &[Point], p: Point) -> bool {
    let mut winding = 0i32;
    for e in ring_edges(points) {
        let o = orient(e.a, e.b, p);
        if o == 0 && Rect::new(e.a, e.b).contains(p) {
            return true;
        }
        if e.a.y <= p.y {
            if e.b.y > p.y && o > 0 {
                winding += 1;
            }
        } else if e.b.y <= p.y && o < 0 {
            winding -= 1;
        }
    }
    winding != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Nm;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn square_with_notch() {
        // A 10×10 square with a 5×5 notch cut from the top-right corner (an L shape).
        let l = Polygon::new(vec![p(0, 0), p(10, 0), p(10, 5), p(5, 5), p(5, 10), p(0, 10)]);
        assert_eq!(l.signed_area_x2(), 150);
        assert_eq!(l.area(), 75.0);
        assert!(l.is_ccw());
        assert!(l.contains(p(2, 2)));
        assert!(l.contains(p(10, 2)));
        assert!(l.contains(p(5, 7)));
        assert!(!l.contains(p(7, 7)));
        assert!(!l.contains(p(11, 2)));
        assert_eq!(l.distance_to_point(p(8, 8)), 3.0);
        assert_eq!(l.bbox(), Some(Rect::new(p(0, 0), p(10, 10))));
        let cw = Polygon::new(l.points.iter().rev().copied().collect());
        assert_eq!(cw.signed_area_x2(), -150);
        assert!(cw.contains(p(2, 2)) && !cw.contains(p(7, 7)));
        assert_eq!(Polygon::default().bbox(), None);
        assert!(!Polygon::default().contains(p(0, 0)));
    }
}
