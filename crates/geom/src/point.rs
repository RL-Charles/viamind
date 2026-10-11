//! Integer points and displacement vectors in nanometres.

use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::units::Nm;

/// A position on the board, in nanometres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: Nm,
    /// Vertical coordinate.
    pub y: Nm,
}

/// A displacement between two points, in nanometres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vector {
    /// Horizontal component.
    pub x: Nm,
    /// Vertical component.
    pub y: Nm,
}

impl Point {
    /// The origin `(0, 0)`.
    pub const ORIGIN: Point = Point::new(Nm::ZERO, Nm::ZERO);

    /// Creates a point from its coordinates.
    pub const fn new(x: Nm, y: Nm) -> Point {
        Point { x, y }
    }

    /// Creates a point from millimetre coordinates, each rounded to the nearest nanometre.
    pub fn from_mm(x: f64, y: f64) -> Point {
        Point::new(Nm::from_mm(x), Nm::from_mm(y))
    }
}

impl Vector {
    /// The zero displacement.
    pub const ZERO: Vector = Vector::new(Nm::ZERO, Nm::ZERO);

    /// Creates a vector from its components.
    pub const fn new(x: Nm, y: Nm) -> Vector {
        Vector { x, y }
    }

    /// Creates a vector from millimetre components, each rounded to the nearest nanometre.
    pub fn from_mm(x: f64, y: f64) -> Vector {
        Vector::new(Nm::from_mm(x), Nm::from_mm(y))
    }
}

/// Moves a point by a vector.
impl Add<Vector> for Point {
    type Output = Point;
    fn add(self, rhs: Vector) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

/// Moves a point by the opposite of a vector.
impl Sub<Vector> for Point {
    type Output = Point;
    fn sub(self, rhs: Vector) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl AddAssign<Vector> for Point {
    fn add_assign(&mut self, rhs: Vector) {
        *self = *self + rhs;
    }
}

impl SubAssign<Vector> for Point {
    fn sub_assign(&mut self, rhs: Vector) {
        *self = *self - rhs;
    }
}

/// The displacement from `rhs` to `self`.
impl Sub for Point {
    type Output = Vector;
    fn sub(self, rhs: Point) -> Vector {
        Vector::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Add for Vector {
    type Output = Vector;
    fn add(self, rhs: Vector) -> Vector {
        Vector::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vector {
    type Output = Vector;
    fn sub(self, rhs: Vector) -> Vector {
        Vector::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Neg for Vector {
    type Output = Vector;
    fn neg(self) -> Vector {
        Vector::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_vector_algebra() {
        let a = Point::from_mm(1.0, 2.0);
        let b = Point::from_mm(4.0, -1.0);
        let v = b - a;
        assert_eq!(v, Vector::from_mm(3.0, -3.0));
        assert_eq!(a + v, b);
        assert_eq!(b - v, a);
        assert_eq!(a + (v - v), a);
        assert_eq!(a + -v + v, a);
        let mut p = a;
        p += v;
        assert_eq!(p, b);
        p -= v;
        assert_eq!(p, a);
        assert_eq!(Point::default(), Point::ORIGIN);
        assert_eq!(Vector::default(), Vector::ZERO);
    }

    #[test]
    fn serde_shape() {
        let p = Point::new(Nm(1), Nm(-2));
        assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"x":1,"y":-2}"#);
    }
}
