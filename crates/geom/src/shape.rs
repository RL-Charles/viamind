//! Filled shapes for copper and keepouts, with clearance between any pair.
//!
//! Every [`Shape`] is a *core* (a point, a segment or a polygon ring) grown by a
//! radius: a circle is a point grown by its radius, a capsule is a segment grown
//! by half its width, rectangles and polygons have radius zero. The clearance
//! between two shapes is the distance between their cores minus both radii.

use serde::{Deserialize, Serialize};

use crate::angle::Angle;
use crate::arc::Arc;
use crate::point::{Point, Vector};
use crate::polygon::{Polygon, ring_contains, ring_edges};
use crate::rect::Rect;
use crate::segment::Segment;
use crate::units::Nm;

/// A filled disc (a via, a round pad).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Circle {
    /// Centre.
    pub center: Point,
    /// Radius.
    pub radius: Nm,
}

/// A stadium: every point within `width / 2` of a segment (a straight track
/// with round ends, an oval pad).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capsule {
    /// The centre line.
    pub segment: Segment,
    /// Full width; the ends are semicircles of diameter `width`.
    pub width: Nm,
}

/// Any filled shape. See the module docs for how clearances are measured.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    /// A disc.
    Circle(Circle),
    /// A segment with width and round ends.
    Capsule(Capsule),
    /// An axis-aligned rectangle.
    Rect(Rect),
    /// A simple polygon.
    Polygon(Polygon),
}

impl Circle {
    /// Creates a circle.
    pub const fn new(center: Point, radius: Nm) -> Circle {
        Circle { center, radius }
    }

    /// Bounding box.
    pub fn bbox(&self) -> Rect {
        Rect::from_point(self.center).inflate(self.radius)
    }

    /// The circle with its radius grown by `d` (shrunk if negative, never below zero).
    pub fn inflate(&self, d: Nm) -> Circle {
        Circle { radius: (self.radius + d).max(Nm::ZERO), ..*self }
    }
}

impl Capsule {
    /// Creates a capsule from its centre line and full width.
    pub const fn new(segment: Segment, width: Nm) -> Capsule {
        Capsule { segment, width }
    }

    /// Bounding box (rounded outwards for odd widths).
    pub fn bbox(&self) -> Rect {
        self.segment.bbox().inflate(Nm((self.width.0 + 1) / 2))
    }

    /// The capsule grown by `d` on every side, so its width grows by `2d` (never below zero).
    pub fn inflate(&self, d: Nm) -> Capsule {
        Capsule { width: (self.width + d * 2).max(Nm::ZERO), ..*self }
    }

    /// Clearance to another capsule (edge to edge), in nanometres; zero if they touch or overlap.
    pub fn clearance(&self, other: &Capsule) -> f64 {
        let d = self.segment.distance_to_segment(&other.segment) - (self.width.0 as f64 + other.width.0 as f64) / 2.0;
        d.max(0.0)
    }
}

/// The skeleton of a shape; see the module docs.
enum Core<'a> {
    Point(Point),
    Segment(Segment),
    Ring(&'a [Point]),
}

impl Core<'_> {
    fn distance(&self, other: &Core<'_>) -> f64 {
        match (self, other) {
            (Core::Point(a), Core::Point(b)) => a.distance(*b),
            (Core::Point(p), Core::Segment(s)) | (Core::Segment(s), Core::Point(p)) => s.distance_to_point(*p),
            (Core::Point(p), Core::Ring(r)) | (Core::Ring(r), Core::Point(p)) => {
                if ring_contains(r, *p) {
                    0.0
                } else {
                    min_dist(ring_edges(r).map(|e| e.distance_to_point(*p)))
                }
            }
            (Core::Segment(a), Core::Segment(b)) => a.distance_to_segment(b),
            (Core::Segment(s), Core::Ring(r)) | (Core::Ring(r), Core::Segment(s)) => {
                if r.is_empty() {
                    f64::INFINITY
                } else if ring_contains(r, s.a) {
                    0.0
                } else {
                    min_dist(ring_edges(r).map(|e| e.distance_to_segment(s)))
                }
            }
            (Core::Ring(a), Core::Ring(b)) => {
                let (Some(&a0), Some(&b0)) = (a.first(), b.first()) else { return f64::INFINITY };
                if ring_contains(a, b0) || ring_contains(b, a0) {
                    return 0.0;
                }
                min_dist(ring_edges(a).flat_map(|ea| ring_edges(b).map(move |eb| ea.distance_to_segment(&eb))))
            }
        }
    }
}

fn min_dist(it: impl Iterator<Item = f64>) -> f64 {
    it.fold(f64::INFINITY, f64::min)
}

impl Shape {
    /// Calls `f` with the shape's core and radius.
    fn with_core<R>(&self, f: impl FnOnce(Core<'_>, f64) -> R) -> R {
        match self {
            Shape::Circle(c) => f(Core::Point(c.center), c.radius.0 as f64),
            Shape::Capsule(c) => f(Core::Segment(c.segment), c.width.0 as f64 / 2.0),
            Shape::Rect(r) => f(Core::Ring(&r.corners()), 0.0),
            Shape::Polygon(p) => f(Core::Ring(&p.points), 0.0),
        }
    }

    /// Bounding box; an empty polygon gives a degenerate box at the origin.
    pub fn bbox(&self) -> Rect {
        match self {
            Shape::Circle(c) => c.bbox(),
            Shape::Capsule(c) => c.bbox(),
            Shape::Rect(r) => *r,
            Shape::Polygon(p) => p.bbox().unwrap_or(Rect::from_point(Point::ORIGIN)),
        }
    }

    /// Distance from `p` to the shape, in nanometres; zero inside or on the boundary.
    pub fn distance_to_point(&self, p: Point) -> f64 {
        self.with_core(|core, r| (core.distance(&Core::Point(p)) - r).max(0.0))
    }

    /// Whether `p` lies inside or on the boundary.
    pub fn contains(&self, p: Point) -> bool {
        self.distance_to_point(p) == 0.0
    }

    /// Clearance (edge-to-edge gap) to `other`, in nanometres; zero if they touch or overlap.
    ///
    /// Polygon–polygon is `O(n·m)` in the vertex counts; use a spatial index to
    /// pick candidate pairs first.
    pub fn distance(&self, other: &Shape) -> f64 {
        self.with_core(|a, ra| other.with_core(|b, rb| (a.distance(&b) - ra - rb).max(0.0)))
    }

    /// Whether the two shapes touch or overlap.
    pub fn intersects(&self, other: &Shape) -> bool {
        self.distance(other) == 0.0
    }

    /// The outline as a counter-clockwise polygon, with round ends flattened.
    ///
    /// Curved edges become chords whose vertices lie on the true outline and
    /// which stay within `tolerance` of it (plus a nanometre of rounding).
    pub fn to_polygon(&self, tolerance: Nm) -> Polygon {
        let open = |mut pts: Vec<Point>| {
            pts.pop();
            pts
        };
        match self {
            Shape::Circle(c) => Polygon::new(open(Arc::new(c.center, c.radius, Angle::ZERO, Angle::FULL).flatten(tolerance))),
            Shape::Capsule(c) => {
                let (a, b) = (c.segment.a, c.segment.b);
                let r = Nm(c.width.0 / 2);
                if a == b {
                    return Shape::Circle(Circle::new(a, r)).to_polygon(tolerance);
                }
                let v = b - a;
                let dir = Angle::from_radians((v.y.0 as f64).atan2(v.x.0 as f64));
                let mut pts = Arc::new(b, r, dir - Angle::QUARTER, Angle::HALF).flatten(tolerance);
                pts.extend(Arc::new(a, r, dir + Angle::QUARTER, Angle::HALF).flatten(tolerance));
                Polygon::new(pts)
            }
            Shape::Rect(r) => Polygon::from_rect(r),
            Shape::Polygon(p) => {
                if p.signed_area_x2() < 0 {
                    Polygon::new(p.points.iter().rev().copied().collect())
                } else {
                    p.clone()
                }
            }
        }
    }

    /// The shape moved by `v`.
    pub fn translate(&self, v: Vector) -> Shape {
        match self {
            Shape::Circle(c) => Shape::Circle(Circle { center: c.center + v, ..*c }),
            Shape::Capsule(c) => Shape::Capsule(Capsule { segment: c.segment.translate(v), ..*c }),
            Shape::Rect(r) => Shape::Rect(r.translate(v)),
            Shape::Polygon(p) => Shape::Polygon(p.translate(v)),
        }
    }
}

impl From<Circle> for Shape {
    fn from(c: Circle) -> Shape {
        Shape::Circle(c)
    }
}

impl From<Capsule> for Shape {
    fn from(c: Capsule) -> Shape {
        Shape::Capsule(c)
    }
}

impl From<Rect> for Shape {
    fn from(r: Rect) -> Shape {
        Shape::Rect(r)
    }
}

impl From<Polygon> for Shape {
    fn from(p: Polygon) -> Shape {
        Shape::Polygon(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn clearances() {
        let via = Shape::from(Circle::new(p(0, 0), Nm(10)));
        let track = Shape::from(Capsule::new(Segment::new(p(-100, 30), p(100, 30)), Nm(10)));
        let pad = Shape::from(Rect::new(p(40, -5), p(60, 5)));
        let tri = Shape::from(Polygon::new(vec![p(-50, -50), p(-20, -50), p(-50, -20)]));
        assert_eq!(via.distance(&track), 15.0);
        assert_eq!(track.distance(&via), 15.0);
        assert_eq!(via.distance(&pad), 30.0);
        assert_eq!(pad.distance(&track), 20.0);
        assert!(via.distance(&tri) > 0.0);
        assert!(via.contains(p(6, 8)));
        assert!(!via.contains(p(7, 8)));
        assert!(track.contains(p(-105, 30)));
        let big = Shape::from(Rect::new(p(-1000, -1000), p(1000, 1000)));
        assert!(big.intersects(&tri) && tri.intersects(&big));
        assert!(big.intersects(&via));
        let a = Capsule::new(Segment::new(p(0, 0), p(100, 0)), Nm(20));
        let b = Capsule::new(Segment::new(p(0, 50), p(100, 50)), Nm(20));
        assert_eq!(a.clearance(&b), 30.0);
        assert_eq!(a.inflate(Nm(15)).clearance(&b), 15.0);
        assert_eq!(a.inflate(Nm(20)).clearance(&b), 10.0);
        assert_eq!(a.inflate(Nm(40)).clearance(&b), 0.0);
    }
}
