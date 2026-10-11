//! Integer angles in nanodegrees.
//!
//! Angles are stored as whole nanodegrees so that arcs, rotations by common
//! angles and undo are exact. At a radius of one metre a nanodegree moves a
//! point by under 0.02 nm, far below the nanometre grid.

use std::ops::{Add, Neg, Sub};

use serde::{Deserialize, Serialize};

/// Nanodegrees per degree.
pub const NANODEG_PER_DEG: i64 = 1_000_000_000;

/// A signed angle in whole nanodegrees.
///
/// Positive angles turn from the +x axis towards the +y axis. Serialises as a
/// bare integer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Angle(pub i64);

impl Angle {
    /// Zero angle.
    pub const ZERO: Angle = Angle(0);
    /// A quarter turn, 90°.
    pub const QUARTER: Angle = Angle(90 * NANODEG_PER_DEG);
    /// A half turn, 180°.
    pub const HALF: Angle = Angle(180 * NANODEG_PER_DEG);
    /// A full turn, 360°.
    pub const FULL: Angle = Angle(360 * NANODEG_PER_DEG);

    /// Creates an angle from degrees, rounded to the nearest nanodegree.
    pub fn from_degrees(deg: f64) -> Angle {
        Angle((deg * NANODEG_PER_DEG as f64).round() as i64)
    }

    /// Creates an angle from radians, rounded to the nearest nanodegree.
    pub fn from_radians(rad: f64) -> Angle {
        Angle::from_degrees(rad.to_degrees())
    }

    /// Returns the angle in degrees.
    pub fn to_degrees(self) -> f64 {
        self.0 as f64 / NANODEG_PER_DEG as f64
    }

    /// Returns the angle in radians.
    pub fn to_radians(self) -> f64 {
        self.to_degrees().to_radians()
    }

    /// The same direction, wrapped into `[0°, 360°)`.
    pub fn normalized(self) -> Angle {
        Angle(self.0.rem_euclid(Angle::FULL.0))
    }

    /// The magnitude of the angle.
    pub fn abs(self) -> Angle {
        Angle(self.0.abs())
    }
}

impl Add for Angle {
    type Output = Angle;
    fn add(self, rhs: Angle) -> Angle {
        Angle(self.0 + rhs.0)
    }
}

impl Sub for Angle {
    type Output = Angle;
    fn sub(self, rhs: Angle) -> Angle {
        Angle(self.0 - rhs.0)
    }
}

impl Neg for Angle {
    type Output = Angle;
    fn neg(self) -> Angle {
        Angle(-self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_and_wrapping() {
        assert_eq!(Angle::from_degrees(90.0), Angle::QUARTER);
        assert_eq!(Angle::from_radians(std::f64::consts::PI), Angle::HALF);
        assert_eq!(Angle::from_degrees(-90.0).normalized(), Angle::from_degrees(270.0));
        assert_eq!((Angle::FULL + Angle::QUARTER).normalized(), Angle::QUARTER);
        assert_eq!(Angle::FULL.normalized(), Angle::ZERO);
        assert!((Angle::HALF.to_radians() - std::f64::consts::PI).abs() < 1e-12);
        assert_eq!(serde_json::to_string(&Angle::QUARTER).unwrap(), "90000000000");
    }
}
