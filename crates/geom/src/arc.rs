//! Circular arcs stored as integers.

use serde::{Deserialize, Serialize};

use crate::angle::Angle;
use crate::point::{Point, Vector};
use crate::rect::Rect;
use crate::segment::orient;
use crate::units::Nm;

/// A circular arc: centre, radius, start angle and signed sweep.
///
/// Every field is an integer, so moving an arc (or turning it by a multiple of
/// 90°) is exact. Positive `sweep` runs counter-clockwise (from +x towards +y);
/// sweeps beyond ±360° are treated as ±360° (a full circle). Points on the arc are computed in
/// `f64` and rounded to the nearest nanometre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Arc {
    /// Centre of the circle.
    pub center: Point,
    /// Radius of the circle.
    pub radius: Nm,
    /// Angle of the start point, measured at the centre from +x.
    pub start: Angle,
    /// Signed angle swept from the start point to the end point.
    pub sweep: Angle,
}

impl Arc {
    /// Creates an arc from its centre, radius, start angle and signed sweep.
    pub const fn new(center: Point, radius: Nm, start: Angle, sweep: Angle) -> Arc {
        Arc { center, radius, start, sweep }
    }

    /// The arc that starts at `start`, passes through `mid` and ends at `end`.
    ///
    /// Returns `None` if the three points are collinear (or coincide). The
    /// centre and radius are rounded to the nearest nanometre, so the stored
    /// endpoints can differ from the inputs by about a nanometre.
    pub fn from_three_points(start: Point, mid: Point, end: Point) -> Option<Arc> {
        let d = orient(start, mid, end);
        if d == 0 {
            return None;
        }
        // Circumcentre relative to `start`, in f64 (inputs are exact integers).
        let (bx, by) = ((mid.x.0 - start.x.0) as f64, (mid.y.0 - start.y.0) as f64);
        let (cx, cy) = ((end.x.0 - start.x.0) as f64, (end.y.0 - start.y.0) as f64);
        let d2 = 2.0 * d as f64;
        let (b2, c2) = (bx * bx + by * by, cx * cx + cy * cy);
        let ux = (cy * b2 - by * c2) / d2;
        let uy = (bx * c2 - cx * b2) / d2;
        let center = start + Vector::new(Nm(ux.round() as i64), Nm(uy.round() as i64));
        let radius = Nm(ux.hypot(uy).round() as i64);
        let angle_of = |p: Point| Angle::from_radians(((p.y.0 - center.y.0) as f64).atan2((p.x.0 - center.x.0) as f64));
        let a0 = angle_of(start);
        let span = (angle_of(end) - a0).normalized();
        // Counter-clockwise when start → mid → end turns left.
        let sweep = if d > 0 { span } else { span - Angle::FULL };
        Some(Arc::new(center, radius, a0, sweep))
    }

    /// The sweep limited to ±360°.
    fn span(&self) -> Angle {
        Angle(self.sweep.0.clamp(-Angle::FULL.0, Angle::FULL.0))
    }

    /// Angle of the end point (`start + sweep`, not normalised).
    pub fn end_angle(&self) -> Angle {
        self.start + self.span()
    }

    /// The point of the circle at angle `a`, rounded to the nearest nanometre.
    pub fn point_at(&self, a: Angle) -> Point {
        let (sin, cos) = a.to_radians().sin_cos();
        let r = self.radius.0 as f64;
        self.center + Vector::new(Nm((r * cos).round() as i64), Nm((r * sin).round() as i64))
    }

    /// The start point.
    pub fn start_point(&self) -> Point {
        self.point_at(self.start)
    }

    /// The end point.
    pub fn end_point(&self) -> Point {
        self.point_at(self.end_angle())
    }

    /// The point halfway along the arc.
    pub fn mid_point(&self) -> Point {
        self.point_at(self.start + Angle(self.span().0 / 2))
    }

    /// Whether the direction `a` falls within the swept range (ends included).
    pub fn contains_angle(&self, a: Angle) -> bool {
        if self.sweep.abs() >= Angle::FULL {
            return true;
        }
        if self.sweep.0 >= 0 { (a - self.start).normalized() <= self.sweep } else { (self.start - a).normalized() <= -self.sweep }
    }

    /// Length along the arc, in nanometres.
    pub fn length(&self) -> f64 {
        self.radius.0 as f64 * self.span().abs().to_radians()
    }

    /// The tight bounding box: endpoints plus any axis extremes the arc passes.
    pub fn bbox(&self) -> Rect {
        let r = self.radius;
        let c = self.center;
        let extremes = [
            (Angle::ZERO, Point::new(c.x + r, c.y)),
            (Angle::QUARTER, Point::new(c.x, c.y + r)),
            (Angle::HALF, Point::new(c.x - r, c.y)),
            (Angle::HALF + Angle::QUARTER, Point::new(c.x, c.y - r)),
        ];
        let mut rect = Rect::new(self.start_point(), self.end_point());
        for (a, p) in extremes {
            if self.contains_angle(a) {
                rect = rect.union(&Rect::from_point(p));
            }
        }
        rect
    }

    /// Distance from `p` to the nearest point of the arc, in nanometres.
    pub fn distance_to_point(&self, p: Point) -> f64 {
        let v = p - self.center;
        let d = v.length();
        if d > 0.0 && self.contains_angle(Angle::from_radians((v.y.0 as f64).atan2(v.x.0 as f64))) {
            return (d - self.radius.0 as f64).abs();
        }
        if d == 0.0 && self.sweep != Angle::ZERO {
            return self.radius.0 as f64;
        }
        self.start_point().distance(p).min(self.end_point().distance(p))
    }

    /// Approximates the arc by a polyline from start to end.
    ///
    /// Each chord stays within `tolerance` of the true arc (plus up to a
    /// nanometre of rounding). Always returns at least the two endpoints.
    pub fn flatten(&self, tolerance: Nm) -> Vec<Point> {
        let r = self.radius.0 as f64;
        let sweep = self.span().0;
        let ratio = (tolerance.0.max(1) as f64 / r.max(1.0)).min(1.0);
        let max_step = 2.0 * (1.0 - ratio).acos();
        let n = ((Angle(sweep).abs().to_radians() / max_step).ceil() as i64).max(1);
        (0..=n).map(|i| self.point_at(self.start + Angle((i128::from(sweep) * i128::from(i) / i128::from(n)) as i64))).collect()
    }

    /// The arc moved by `v`.
    pub fn translate(&self, v: Vector) -> Arc {
        Arc { center: self.center + v, ..*self }
    }

    /// The same arc traversed from end to start.
    pub fn reversed(&self) -> Arc {
        Arc { start: self.end_angle(), sweep: -self.span(), ..*self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn quarter_arc() {
        let a = Arc::new(Point::ORIGIN, Nm(1000), Angle::ZERO, Angle::QUARTER);
        assert_eq!(a.start_point(), p(1000, 0));
        assert_eq!(a.end_point(), p(0, 1000));
        assert_eq!(a.bbox(), Rect::new(p(0, 0), p(1000, 1000)));
        assert!(a.contains_angle(Angle::from_degrees(45.0)));
        assert!(!a.contains_angle(Angle::from_degrees(-1.0)));
        assert_eq!(a.distance_to_point(p(2000, 0)), 1000.0);
        assert_eq!(a.distance_to_point(p(0, -1000)), 1000.0_f64.hypot(1000.0));
        let cw = a.reversed();
        assert_eq!(cw.start_point(), p(0, 1000));
        assert_eq!(cw.bbox(), a.bbox());
        let full = Arc::new(p(5, 5), Nm(10), Angle::QUARTER, -Angle::FULL);
        assert_eq!(full.bbox(), Rect::new(p(-5, -5), p(15, 15)));
    }

    #[test]
    fn three_points() {
        let a = Arc::from_three_points(p(1000, 0), p(0, 1000), p(-1000, 0)).unwrap();
        assert_eq!((a.center, a.radius, a.start, a.sweep), (Point::ORIGIN, Nm(1000), Angle::ZERO, Angle::HALF));
        let b = Arc::from_three_points(p(-1000, 0), p(0, 1000), p(1000, 0)).unwrap();
        assert_eq!(b.sweep, -Angle::HALF);
        assert_eq!(b.end_point(), p(1000, 0));
        assert!(Arc::from_three_points(p(0, 0), p(1, 1), p(2, 2)).is_none());
    }

    #[test]
    fn flatten_ends_on_endpoints() {
        let a = Arc::new(p(3, 4), Nm(1_000_000), Angle::from_degrees(10.0), Angle::from_degrees(-200.0));
        let pts = a.flatten(Nm(1000));
        assert!(pts.len() > 2);
        assert_eq!(pts.first(), Some(&a.start_point()));
        assert_eq!(pts.last(), Some(&a.end_point()));
    }
}
