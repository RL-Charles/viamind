//! Axis-aligned bounding boxes.

use serde::{Deserialize, Serialize};

use crate::point::{Point, Vector};
use crate::units::Nm;

/// A closed axis-aligned rectangle in nanometres.
///
/// Invariant: `min.x <= max.x` and `min.y <= max.y`. The constructors keep it;
/// code that writes the fields directly must too. A rectangle may be degenerate
/// (zero width or height), and its edges belong to it: rectangles that only
/// touch still intersect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    /// Lower-left corner (smallest x and y).
    pub min: Point,
    /// Upper-right corner (largest x and y).
    pub max: Point,
}

impl Rect {
    /// Creates the rectangle spanned by two opposite corners, in any order.
    pub fn new(a: Point, b: Point) -> Rect {
        Rect { min: Point::new(a.x.min(b.x), a.y.min(b.y)), max: Point::new(a.x.max(b.x), a.y.max(b.y)) }
    }

    /// The degenerate rectangle holding a single point.
    pub const fn from_point(p: Point) -> Rect {
        Rect { min: p, max: p }
    }

    /// The smallest rectangle holding all `points`, or `None` if there are none.
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut it = points.into_iter();
        let first = Rect::from_point(it.next()?);
        Some(it.fold(first, |r, p| r.union(&Rect::from_point(p))))
    }

    /// Extent along x.
    pub fn width(&self) -> Nm {
        self.max.x - self.min.x
    }

    /// Extent along y.
    pub fn height(&self) -> Nm {
        self.max.y - self.min.y
    }

    /// The centre, rounded down to the nanometre grid.
    pub fn center(&self) -> Point {
        Point::new(Nm(self.min.x.0 + self.width().0 / 2), Nm(self.min.y.0 + self.height().0 / 2))
    }

    /// The four corners, counter-clockwise from `min`.
    pub fn corners(&self) -> [Point; 4] {
        [self.min, Point::new(self.max.x, self.min.y), self.max, Point::new(self.min.x, self.max.y)]
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// The overlap of both, or `None` if they are disjoint.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let min = Point::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y));
        let max = Point::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y));
        (min.x <= max.x && min.y <= max.y).then_some(Rect { min, max })
    }

    /// Whether the two share at least one point (touching counts).
    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x <= other.max.x && other.min.x <= self.max.x && self.min.y <= other.max.y && other.min.y <= self.max.y
    }

    /// Whether `p` lies inside or on the boundary.
    pub fn contains(&self, p: Point) -> bool {
        self.min.x <= p.x && p.x <= self.max.x && self.min.y <= p.y && p.y <= self.max.y
    }

    /// Whether `other` lies entirely inside (boundaries may coincide).
    pub fn contains_rect(&self, other: &Rect) -> bool {
        self.contains(other.min) && self.contains(other.max)
    }

    /// Grows every side outwards by `d`; a negative `d` shrinks it.
    ///
    /// Shrinking past zero size collapses that axis onto the centre line rather
    /// than inverting the rectangle.
    pub fn inflate(&self, d: Nm) -> Rect {
        let axis = |lo: Nm, hi: Nm| {
            let (lo, hi) = (lo - d, hi + d);
            if lo <= hi {
                (lo, hi)
            } else {
                let mid = Nm(lo.0 + (hi.0 - lo.0) / 2);
                (mid, mid)
            }
        };
        let (x0, x1) = axis(self.min.x, self.max.x);
        let (y0, y1) = axis(self.min.y, self.max.y);
        Rect { min: Point::new(x0, y0), max: Point::new(x1, y1) }
    }

    /// The rectangle moved by `v`.
    pub fn translate(&self, v: Vector) -> Rect {
        Rect { min: self.min + v, max: self.max + v }
    }

    /// Distance from `p` to the nearest point of the rectangle, in nanometres; zero inside.
    pub fn distance_to_point(&self, p: Point) -> f64 {
        let dx = (self.min.x.0 - p.x.0).max(p.x.0 - self.max.x.0).max(0);
        let dy = (self.min.y.0 - p.y.0).max(p.y.0 - self.max.y.0).max(0);
        (dx as f64).hypot(dy as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
        Rect::new(Point::new(Nm(x0), Nm(y0)), Point::new(Nm(x1), Nm(y1)))
    }

    #[test]
    fn basics() {
        let a = r(10, 10, 0, 0);
        assert_eq!(a.min, Point::ORIGIN);
        assert_eq!(a.width(), Nm(10));
        assert_eq!(a.center(), Point::new(Nm(5), Nm(5)));
        let b = r(10, 0, 20, 5);
        assert!(a.intersects(&b));
        assert_eq!(a.intersection(&b), Some(r(10, 0, 10, 5)));
        assert!(!a.intersects(&r(11, 0, 20, 5)));
        assert_eq!(a.union(&b), r(0, 0, 20, 10));
        assert_eq!(a.inflate(Nm(2)), r(-2, -2, 12, 12));
        assert_eq!(a.inflate(Nm(-7)), r(5, 5, 5, 5));
        assert_eq!(a.distance_to_point(Point::new(Nm(13), Nm(14))), 5.0);
        assert_eq!(Rect::from_points([Point::ORIGIN, Point::new(Nm(3), Nm(-2))]), Some(r(0, -2, 3, 0)));
        assert_eq!(Rect::from_points([]), None);
    }
}
