//! Integer nanometre lengths and unit conversions.
//!
//! All board geometry in Viamind is stored as whole nanometres in an `i64`, so
//! moves, copies and undo are exact: adding and then subtracting the same
//! offset always returns to the original coordinate. Floating-point values only
//! appear at the edges (user input, display, file import/export) through the
//! conversion functions here.

use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Nanometres per micrometre.
pub const NM_PER_UM: i64 = 1_000;
/// Nanometres per millimetre.
pub const NM_PER_MM: i64 = 1_000_000;
/// Nanometres per mil (one thousandth of an inch).
pub const NM_PER_MIL: i64 = 25_400;
/// Nanometres per inch.
pub const NM_PER_INCH: i64 = 25_400_000;

/// A signed length in whole nanometres.
///
/// The `i64` range covers about ±9.2 million km, far beyond any board, while a
/// nanometre is well below any manufacturing tolerance. Serialises as a bare
/// integer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Nm(pub i64);

impl Nm {
    /// Zero length.
    pub const ZERO: Nm = Nm(0);

    /// Converts `value` in units of `nm_per_unit` nanometres, rounding to the nearest nanometre.
    fn from_unit(value: f64, nm_per_unit: i64) -> Nm {
        Nm((value * nm_per_unit as f64).round() as i64)
    }

    /// Creates a length from millimetres, rounded to the nearest nanometre.
    pub fn from_mm(mm: f64) -> Nm {
        Nm::from_unit(mm, NM_PER_MM)
    }

    /// Creates a length from mils (thousandths of an inch), rounded to the nearest nanometre.
    pub fn from_mil(mil: f64) -> Nm {
        Nm::from_unit(mil, NM_PER_MIL)
    }

    /// Creates a length from inches, rounded to the nearest nanometre.
    pub fn from_inch(inch: f64) -> Nm {
        Nm::from_unit(inch, NM_PER_INCH)
    }

    /// Creates a length from micrometres, rounded to the nearest nanometre.
    pub fn from_um(um: f64) -> Nm {
        Nm::from_unit(um, NM_PER_UM)
    }

    /// Returns the length in millimetres.
    pub fn to_mm(self) -> f64 {
        self.0 as f64 / NM_PER_MM as f64
    }

    /// Returns the length in mils (thousandths of an inch).
    pub fn to_mil(self) -> f64 {
        self.0 as f64 / NM_PER_MIL as f64
    }

    /// Returns the length in inches.
    pub fn to_inch(self) -> f64 {
        self.0 as f64 / NM_PER_INCH as f64
    }
}

impl Add for Nm {
    type Output = Nm;
    fn add(self, rhs: Nm) -> Nm {
        Nm(self.0 + rhs.0)
    }
}

impl Sub for Nm {
    type Output = Nm;
    fn sub(self, rhs: Nm) -> Nm {
        Nm(self.0 - rhs.0)
    }
}

impl Neg for Nm {
    type Output = Nm;
    fn neg(self) -> Nm {
        Nm(-self.0)
    }
}

impl AddAssign for Nm {
    fn add_assign(&mut self, rhs: Nm) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Nm {
    fn sub_assign(&mut self, rhs: Nm) {
        self.0 -= rhs.0;
    }
}

impl Mul<i64> for Nm {
    type Output = Nm;
    fn mul(self, rhs: i64) -> Nm {
        Nm(self.0 * rhs)
    }
}

/// Integer division, truncating toward zero like `i64` division.
impl Div<i64> for Nm {
    type Output = Nm;
    fn div(self, rhs: i64) -> Nm {
        Nm(self.0 / rhs)
    }
}

impl Sum for Nm {
    fn sum<I: Iterator<Item = Nm>>(iter: I) -> Nm {
        iter.fold(Nm::ZERO, Add::add)
    }
}

impl<'a> Sum<&'a Nm> for Nm {
    fn sum<I: Iterator<Item = &'a Nm>>(iter: I) -> Nm {
        iter.copied().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(Nm::from_mm(1.0), Nm(NM_PER_MM));
        assert_eq!(Nm::from_mil(1.0), Nm(NM_PER_MIL));
        assert_eq!(Nm::from_inch(1.0), Nm(NM_PER_INCH));
        assert_eq!(Nm::from_um(1.0), Nm(NM_PER_UM));
        assert_eq!(Nm::from_mil(1000.0), Nm::from_inch(1.0));
        assert_eq!(Nm::from_inch(1.0), Nm::from_mm(25.4));
    }

    #[test]
    fn rounds_to_nearest() {
        assert_eq!(Nm::from_mm(0.000_000_4), Nm(0));
        assert_eq!(Nm::from_mm(0.000_000_6), Nm(1));
        assert_eq!(Nm::from_mm(-0.000_000_6), Nm(-1));
        assert_eq!(Nm::from_um(1.4999), Nm(1500));
    }

    #[test]
    fn arithmetic() {
        let a = Nm(10);
        let b = Nm(3);
        assert_eq!(a + b, Nm(13));
        assert_eq!(a - b, Nm(7));
        assert_eq!(-a, Nm(-10));
        assert_eq!(a * 3, Nm(30));
        assert_eq!(a / 3, Nm(3));
        assert_eq!(Nm(-10) / 3, Nm(-3));
        let mut c = a;
        c += b;
        c -= Nm(1);
        assert_eq!(c, Nm(12));
        assert_eq!([a, b, c].iter().sum::<Nm>(), Nm(25));
        assert_eq!([a, b].into_iter().sum::<Nm>(), Nm(13));
        assert!(b < a);
        assert_eq!(Nm::default(), Nm::ZERO);
    }

    #[test]
    fn serde_is_bare_integer() {
        let json = serde_json::to_string(&Nm(-1_234_567)).unwrap();
        assert_eq!(json, "-1234567");
        assert_eq!(serde_json::from_str::<Nm>(&json).unwrap(), Nm(-1_234_567));
    }
}
