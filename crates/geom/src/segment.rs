//! Line segments with exact intersection tests.
//!
//! Predicates (orientation, intersection) are computed exactly in `i128`, so
//! they never disagree with each other because of rounding. Distances are
//! returned as `f64` nanometres; they are accurate to well under a nanometre
//! for any board-sized coordinates.

use serde::{Deserialize, Serialize};

use crate::point::{Point, Vector};
use crate::rect::Rect;
use crate::units::Nm;

/// Twice the signed area of triangle `o, a, b`: positive if `o → a → b` turns
/// counter-clockwise, negative if clockwise, zero if collinear. Exact.
pub(crate) fn orient(o: Point, a: Point, b: Point) -> i128 {
    let (ax, ay) = (i128::from(a.x.0) - i128::from(o.x.0), i128::from(a.y.0) - i128::from(o.y.0));
    let (bx, by) = (i128::from(b.x.0) - i128::from(o.x.0), i128::from(b.y.0) - i128::from(o.y.0));
    ax * by - ay * bx
}

/// Dot product of `a - o` and `b - o`. Exact.
fn dot(o: Point, a: Point, b: Point) -> i128 {
    let (ax, ay) = (i128::from(a.x.0) - i128::from(o.x.0), i128::from(a.y.0) - i128::from(o.y.0));
    let (bx, by) = (i128::from(b.x.0) - i128::from(o.x.0), i128::from(b.y.0) - i128::from(o.y.0));
    ax * bx + ay * by
}

/// Whether `p`, known to be collinear with `s`, lies within its extent.
fn on_collinear(s: &Segment, p: Point) -> bool {
    s.a.x.min(s.b.x) <= p.x && p.x <= s.a.x.max(s.b.x) && s.a.y.min(s.b.y) <= p.y && p.y <= s.a.y.max(s.b.y)
}

/// Integer division rounding to the nearest integer, ties away from zero.
fn div_round(n: i128, d: i128) -> i128 {
    let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
    if n >= 0 { (n + d / 2) / d } else { (n - d / 2) / d }
}

/// A straight line segment between two points.
///
/// A segment whose endpoints coincide is a single point and behaves as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Segment {
    /// Start point.
    pub a: Point,
    /// End point.
    pub b: Point,
}

/// How two segments meet; see [`Segment::intersection`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentIntersection {
    /// The segments do not meet.
    None,
    /// The segments meet in one point, rounded to the nearest nanometre.
    Point(Point),
    /// The segments are collinear and share this stretch (exact).
    Overlap(Segment),
}

impl Segment {
    /// Creates a segment from `a` to `b`.
    pub const fn new(a: Point, b: Point) -> Segment {
        Segment { a, b }
    }

    /// The displacement from `a` to `b`.
    pub fn vector(&self) -> Vector {
        self.b - self.a
    }

    /// Length in nanometres.
    pub fn length(&self) -> f64 {
        self.vector().length()
    }

    /// Bounding box of both endpoints.
    pub fn bbox(&self) -> Rect {
        Rect::new(self.a, self.b)
    }

    /// The segment moved by `v`.
    pub fn translate(&self, v: Vector) -> Segment {
        Segment::new(self.a + v, self.b + v)
    }

    /// The same segment from `b` to `a`.
    pub fn reversed(&self) -> Segment {
        Segment::new(self.b, self.a)
    }

    /// Distance from `p` to the nearest point of the segment, in nanometres.
    pub fn distance_to_point(&self, p: Point) -> f64 {
        let len2 = dot(self.a, self.b, self.b);
        if len2 == 0 || dot(self.a, self.b, p) <= 0 {
            return self.a.distance(p);
        }
        if dot(self.b, self.a, p) <= 0 {
            return self.b.distance(p);
        }
        // Interior: perpendicular distance from the exact cross product.
        (orient(self.a, self.b, p).abs() as f64) / (len2 as f64).sqrt()
    }

    /// The point of the segment nearest to `p`, rounded to the nearest nanometre.
    pub fn closest_point(&self, p: Point) -> Point {
        let len2 = dot(self.a, self.b, self.b);
        let t = dot(self.a, self.b, p);
        if len2 == 0 || t <= 0 {
            return self.a;
        }
        if t >= len2 {
            return self.b;
        }
        let v = self.vector();
        let along = |c: Nm| Nm(div_round(i128::from(c.0) * t, len2) as i64);
        self.a + Vector::new(along(v.x), along(v.y))
    }

    /// Whether the two segments share at least one point (touching counts). Exact.
    pub fn intersects(&self, other: &Segment) -> bool {
        let (p, q) = (self, other);
        let d1 = orient(q.a, q.b, p.a);
        let d2 = orient(q.a, q.b, p.b);
        let d3 = orient(p.a, p.b, q.a);
        let d4 = orient(p.a, p.b, q.b);
        if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
            return true;
        }
        (d1 == 0 && on_collinear(q, p.a))
            || (d2 == 0 && on_collinear(q, p.b))
            || (d3 == 0 && on_collinear(p, q.a))
            || (d4 == 0 && on_collinear(p, q.b))
    }

    /// Where the two segments meet.
    ///
    /// Collinear overlaps are reported exactly; a single crossing point is
    /// rounded to the nearest nanometre.
    pub fn intersection(&self, other: &Segment) -> SegmentIntersection {
        if !self.intersects(other) {
            return SegmentIntersection::None;
        }
        let r = self.vector();
        let s = other.vector();
        let denom = i128::from(r.x.0) * i128::from(s.y.0) - i128::from(r.y.0) * i128::from(s.x.0);
        if denom == 0 {
            // Parallel and touching, so collinear (or degenerate): overlap along the line.
            let key = |p: Point| (p.x, p.y);
            let (lo1, hi1) = if key(self.a) <= key(self.b) { (self.a, self.b) } else { (self.b, self.a) };
            let (lo2, hi2) = if key(other.a) <= key(other.b) { (other.a, other.b) } else { (other.b, other.a) };
            let lo = if key(lo1) >= key(lo2) { lo1 } else { lo2 };
            let hi = if key(hi1) <= key(hi2) { hi1 } else { hi2 };
            return if lo == hi { SegmentIntersection::Point(lo) } else { SegmentIntersection::Overlap(Segment::new(lo, hi)) };
        }
        // self.a + r * t with t = ((other.a - self.a) × s) / (r × s).
        let w = other.a - self.a;
        let num = i128::from(w.x.0) * i128::from(s.y.0) - i128::from(w.y.0) * i128::from(s.x.0);
        let along = |c: Nm| Nm(div_round(i128::from(c.0) * num, denom) as i64);
        SegmentIntersection::Point(self.a + Vector::new(along(r.x), along(r.y)))
    }

    /// Distance between the nearest points of the two segments, in nanometres; zero if they meet.
    pub fn distance_to_segment(&self, other: &Segment) -> f64 {
        if self.intersects(other) {
            return 0.0;
        }
        self.distance_to_point(other.a).min(self.distance_to_point(other.b)).min(other.distance_to_point(self.a)).min(other.distance_to_point(self.b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    fn s(ax: i64, ay: i64, bx: i64, by: i64) -> Segment {
        Segment::new(p(ax, ay), p(bx, by))
    }

    #[test]
    fn point_distance() {
        let seg = s(0, 0, 10, 0);
        assert_eq!(seg.distance_to_point(p(5, 3)), 3.0);
        assert_eq!(seg.distance_to_point(p(-3, 4)), 5.0);
        assert_eq!(seg.distance_to_point(p(13, -4)), 5.0);
        assert_eq!(seg.closest_point(p(5, 3)), p(5, 0));
        assert_eq!(s(1, 1, 1, 1).distance_to_point(p(4, 5)), 5.0);
    }

    #[test]
    fn crossings() {
        assert_eq!(s(0, 0, 10, 10).intersection(&s(0, 10, 10, 0)), SegmentIntersection::Point(p(5, 5)));
        assert_eq!(s(0, 0, 10, 0).intersection(&s(10, 0, 10, 5)), SegmentIntersection::Point(p(10, 0)));
        assert_eq!(s(0, 0, 10, 0).intersection(&s(5, 0, 20, 0)), SegmentIntersection::Overlap(s(5, 0, 10, 0)));
        assert_eq!(s(0, 0, 10, 0).intersection(&s(11, 0, 20, 0)), SegmentIntersection::None);
        assert_eq!(s(0, 0, 10, 0).intersection(&s(0, 1, 10, 1)), SegmentIntersection::None);
        assert_eq!(s(0, 0, 10, 0).distance_to_segment(&s(0, 1, 10, 1)), 1.0);
        assert_eq!(s(0, 0, 10, 0).distance_to_segment(&s(13, 4, 20, 4)), 5.0);
        // Coordinates near ±10 m: products overflow i64 but not i128.
        let big = 10_000_000_000;
        assert!(s(-big, -big, big, big).intersects(&s(-big, big, big, -big)));
        assert!(!s(-big, -big, big, big).intersects(&s(-big, -big + 1, big - 1, big)));
    }
}
