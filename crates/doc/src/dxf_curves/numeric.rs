//! Exact bounded decimals, outward-rounded intervals and certified circular functions.
//!
//! Rust documents IEEE-754 round-to-nearest results for `+`, `-`, `*`, `/`, `sqrt` and
//! fused `mul_add`, but leaves `sin`/`cos`/`atan` precision unspecified. Every bound here
//! therefore uses only the correctly rounded operations, widened outward, plus finite
//! range-reduced Taylor series whose alternating remainder is enclosed explicitly.
use std::cmp::Ordering;

/// Largest written decimal exponent accepted in a curve-entry numeric token.
pub(crate) const MAX_EXPONENT: i32 = 32;
pub(crate) const MAX_SIGNIFICANT_DIGITS: usize = 32;
pub(crate) const MAX_TOKEN_BYTES: usize = 64;
/// Exact fixed-point angle arithmetic supports at most this many fractional degree digits.
pub(crate) const MAX_ANGLE_FRACTION_DIGITS: u32 = 35;

pub(crate) type NumResult<T> = Result<T, &'static str>;

/// Closed interval `[lo, hi]` known to contain an exact real value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Interval {
    pub lo: f64,
    pub hi: f64,
}

fn finite(lo: f64, hi: f64) -> NumResult<Interval> {
    if lo.is_finite() && hi.is_finite() && lo <= hi { Ok(Interval { lo, hi }) } else { Err("numeric enclosure overflowed the f64 range") }
}
/// Products far from underflow have an exactly representable fused-multiply error term.
fn trusted_product(p: f64) -> bool {
    p == 0.0 || p.abs() >= f64::from_bits(0x0770_0000_0000_0000)
}
/// Knuth TwoSum: `a + b = s + err` exactly when no overflow occurs.
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}
fn sum_down(a: f64, b: f64) -> f64 {
    let (s, err) = two_sum(a, b);
    if err < 0.0 || err.is_nan() { s.next_down() } else { s }
}
fn sum_up(a: f64, b: f64) -> f64 {
    let (s, err) = two_sum(a, b);
    if err > 0.0 || err.is_nan() { s.next_up() } else { s }
}
fn product_down(a: f64, b: f64) -> f64 {
    if a == 0.0 || b == 0.0 {
        return 0.0;
    }
    let p = a * b;
    if p == 0.0 {
        // Underflow: the exact product is a tiny value with the operands' sign.
        return if (a < 0.0) == (b < 0.0) { 0.0 } else { p.next_down() };
    }
    let err = a.mul_add(b, -p);
    if !trusted_product(p) || err < 0.0 { p.next_down() } else { p }
}
fn product_up(a: f64, b: f64) -> f64 {
    if a == 0.0 || b == 0.0 {
        return 0.0;
    }
    let p = a * b;
    if p == 0.0 {
        return if (a < 0.0) == (b < 0.0) { p.next_up() } else { 0.0 };
    }
    let err = a.mul_add(b, -p);
    if !trusted_product(p) || err > 0.0 { p.next_up() } else { p }
}

impl Interval {
    pub(crate) const ZERO: Interval = Interval { lo: 0.0, hi: 0.0 };
    pub(crate) const ONE: Interval = Interval { lo: 1.0, hi: 1.0 };
    pub(crate) fn exact(value: f64) -> Interval {
        Interval { lo: value, hi: value }
    }
    /// Enclosure of pi: `f64::consts::PI` is the nearest double below pi.
    pub(crate) fn pi() -> Interval {
        Interval { lo: std::f64::consts::PI, hi: std::f64::consts::PI.next_up() }
    }
    pub(crate) fn add(self, other: Interval) -> NumResult<Interval> {
        finite(sum_down(self.lo, other.lo), sum_up(self.hi, other.hi))
    }
    pub(crate) fn sub(self, other: Interval) -> NumResult<Interval> {
        self.add(other.neg())
    }
    pub(crate) fn neg(self) -> Interval {
        Interval { lo: -self.hi, hi: -self.lo }
    }
    pub(crate) fn mul(self, other: Interval) -> NumResult<Interval> {
        let pairs = [(self.lo, other.lo), (self.lo, other.hi), (self.hi, other.lo), (self.hi, other.hi)];
        let lo = pairs.iter().map(|(a, b)| product_down(*a, *b)).fold(f64::INFINITY, f64::min);
        let hi = pairs.iter().map(|(a, b)| product_up(*a, *b)).fold(f64::NEG_INFINITY, f64::max);
        finite(lo, hi)
    }
    pub(crate) fn scale(self, factor: f64) -> NumResult<Interval> {
        self.mul(Interval::exact(factor))
    }
    /// Division by an interval excluding zero; correctly rounded quotients are widened one ulp.
    pub(crate) fn div(self, other: Interval) -> NumResult<Interval> {
        if other.lo <= 0.0 && other.hi >= 0.0 {
            return Err("numeric enclosure divides by an interval containing zero");
        }
        let q = [self.lo / other.lo, self.lo / other.hi, self.hi / other.lo, self.hi / other.hi];
        finite(q.iter().copied().fold(f64::INFINITY, f64::min).next_down(), q.iter().copied().fold(f64::NEG_INFINITY, f64::max).next_up())
    }
    /// Square root of a nonnegative enclosure; `f64::sqrt` is correctly rounded.
    pub(crate) fn sqrt(self) -> NumResult<Interval> {
        if self.lo < 0.0 {
            return Err("numeric enclosure takes the square root of a possibly negative value");
        }
        let root = |v: f64, up: bool| {
            let r = v.sqrt();
            let err = r.mul_add(r, -v);
            if !trusted_product(v) || (up && err < 0.0) || (!up && err > 0.0) { if up { r.next_up() } else { r.next_down().max(0.0) } } else { r }
        };
        finite(root(self.lo, false), root(self.hi, true))
    }
    pub(crate) fn square(self) -> NumResult<Interval> {
        let m = self.abs();
        m.mul(m)
    }
    /// Exact image under absolute value.
    pub(crate) fn abs(self) -> Interval {
        if self.lo >= 0.0 {
            self
        } else if self.hi <= 0.0 {
            self.neg()
        } else {
            Interval { lo: 0.0, hi: (-self.lo).max(self.hi) }
        }
    }
    pub(crate) fn magnitude(self) -> f64 {
        self.lo.abs().max(self.hi.abs())
    }
    pub(crate) fn width(self) -> f64 {
        sum_up(self.hi, -self.lo)
    }
    pub(crate) fn from_u128(value: u128) -> Interval {
        let f = value as f64;
        // `as` rounds to nearest; saturating back-conversion detects every inexact case.
        if f < 3.4e38 && f as u128 == value { Interval::exact(f) } else { Interval { lo: f.next_down(), hi: f.next_up() } }
    }
    pub(crate) fn from_i128(value: i128) -> Interval {
        let m = Interval::from_u128(value.unsigned_abs());
        if value < 0 { m.neg() } else { m }
    }
    /// Enclosure of `10^k`.
    pub(crate) fn pow10(k: u32) -> NumResult<Interval> {
        if k <= 22 {
            return Ok(Interval::exact(10f64.powi(k as i32)));
        }
        let mut result = Interval::exact(1e22);
        let mut remaining = k - 22;
        while remaining > 0 {
            let step = remaining.min(22);
            result = result.mul(Interval::exact(10f64.powi(step as i32)))?;
            remaining -= step;
        }
        Ok(result)
    }
    /// Enclosure of `numerator * 10^exponent`.
    pub(crate) fn decimal(neg: bool, numerator: u128, exponent: i32) -> NumResult<Interval> {
        let magnitude = if exponent >= 0 {
            match 10u128.checked_pow(exponent as u32).and_then(|p| numerator.checked_mul(p)) {
                Some(v) => Interval::from_u128(v),
                None => Interval::from_u128(numerator).mul(Interval::pow10(exponent as u32)?)?,
            }
        } else {
            let k = exponent.unsigned_abs();
            match 10u128.checked_pow(k) {
                Some(p) => {
                    let (q, r) = (numerator / p, numerator % p);
                    Interval::from_u128(q).add(Interval::from_u128(r).div(Interval::from_u128(p))?)?
                }
                None => Interval::from_u128(numerator).div(Interval::pow10(k)?)?,
            }
        };
        Ok(if neg { magnitude.neg() } else { magnitude })
    }
}

/// Exact source decimal `(-1)^neg * mant * 10^exp` with trailing zeros folded into `exp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    pub neg: bool,
    pub mant: u128,
    pub exp: i32,
}

impl Decimal {
    /// Strict bounded decimal grammar: sign, digits, optional point, optional decimal exponent.
    pub(crate) fn parse(token: &str) -> Result<Decimal, String> {
        let short = || token.chars().take(24).collect::<String>().escape_debug().to_string();
        if token.len() > MAX_TOKEN_BYTES {
            return Err(format!("numeric token exceeds {MAX_TOKEN_BYTES} bytes"));
        }
        let bytes = token.as_bytes();
        let mut i = 0;
        let neg = match bytes.first() {
            Some(b'-') => {
                i += 1;
                true
            }
            Some(b'+') => {
                i += 1;
                false
            }
            _ => false,
        };
        let mut digits = Vec::new();
        let mut fraction = 0i32;
        let mut point = false;
        while let Some(&b) = bytes.get(i) {
            match b {
                b'0'..=b'9' => {
                    digits.push(b - b'0');
                    if point {
                        fraction += 1;
                    }
                }
                b'.' if !point => point = true,
                _ => break,
            }
            i += 1;
        }
        if digits.is_empty() {
            return Err(format!("invalid decimal number {}", short()));
        }
        let mut written = 0i32;
        if matches!(bytes.get(i), Some(b'e' | b'E')) {
            i += 1;
            let negative = match bytes.get(i) {
                Some(b'-') => {
                    i += 1;
                    true
                }
                Some(b'+') => {
                    i += 1;
                    false
                }
                _ => false,
            };
            let start = i;
            while let Some(b @ b'0'..=b'9') = bytes.get(i) {
                written = written.saturating_mul(10).saturating_add(i32::from(b - b'0'));
                i += 1;
            }
            if i == start {
                return Err(format!("invalid decimal exponent in {}", short()));
            }
            if negative {
                written = -written;
            }
            if !(-MAX_EXPONENT..=MAX_EXPONENT).contains(&written) {
                return Err(format!("decimal exponent outside +/-{MAX_EXPONENT} in {}", short()));
            }
        }
        if i != bytes.len() {
            return Err(format!("invalid decimal number {} (no NaN, infinity or hex)", short()));
        }
        let first = digits.iter().position(|d| *d != 0);
        let Some(first) = first else { return Ok(Decimal { neg: false, mant: 0, exp: 0 }) };
        let last = digits.iter().rposition(|d| *d != 0).unwrap_or(first);
        let significant = &digits[first..=last];
        if significant.len() > MAX_SIGNIFICANT_DIGITS {
            return Err(format!("more than {MAX_SIGNIFICANT_DIGITS} significant digits in {}", short()));
        }
        let mant = significant.iter().fold(0u128, |m, d| m * 10 + u128::from(*d));
        let trailing = (digits.len() - 1 - last) as i32;
        Ok(Decimal { neg, mant, exp: written - fraction + trailing })
    }
    pub(crate) fn is_zero(self) -> bool {
        self.mant == 0
    }
    pub(crate) fn is_one(self) -> bool {
        !self.neg && self.mant == 1 && self.exp == 0
    }
    /// Exact comparison of `|self|` with a nonnegative integer.
    pub(crate) fn cmp_abs_integer(self, value: u128) -> Ordering {
        if self.exp >= 0 {
            match 10u128.checked_pow(self.exp as u32).and_then(|p| self.mant.checked_mul(p)) {
                Some(v) => v.cmp(&value),
                None => Ordering::Greater,
            }
        } else {
            match 10u128.checked_pow(self.exp.unsigned_abs()).and_then(|p| value.checked_mul(p)) {
                Some(rhs) => self.mant.cmp(&rhs),
                None => Ordering::Less,
            }
        }
    }
    /// Enclosure of `self * factor * 10^shift` (an exact unit conversion when given).
    pub(crate) fn scaled(self, factor: u128, shift: i32) -> NumResult<Interval> {
        let mant = self.mant.checked_mul(factor).ok_or("numeric unit conversion overflowed")?;
        Interval::decimal(self.neg, mant, self.exp + shift)
    }
    /// Exact fixed-point value `self * 10^digits`, when integral and representable.
    pub(crate) fn fixed(self, digits: u32) -> Option<i128> {
        let exp = self.exp.checked_add(digits as i32)?;
        if exp < 0 {
            return None;
        }
        let v = 10u128.checked_pow(exp as u32).and_then(|p| self.mant.checked_mul(p)).and_then(|v| i128::try_from(v).ok())?;
        Some(if self.neg { -v } else { v })
    }
    /// Number of fractional decimal digits needed to represent the value exactly.
    pub(crate) fn fraction_digits(self) -> u32 {
        if self.exp < 0 { self.exp.unsigned_abs() } else { 0 }
    }
}

/// Bounded numerical work shared by one conversion.
#[derive(Debug)]
pub(crate) struct Work {
    pub used: u64,
    pub limit: u64,
}
impl Work {
    pub(crate) fn step(&mut self, n: u64) -> NumResult<()> {
        self.used = self.used.checked_add(n).ok_or("numerical work counter overflowed")?;
        if self.used > self.limit { Err("numerical refinement/evaluation work limit exhausted") } else { Ok(()) }
    }
}

const SERIES_TERMS: u32 = 12;

/// Certified `(cos x, sin x)` for an enclosure `0 <= x <= 0.8` (covers `[0, pi/4]`).
///
/// Both Taylor series alternate with strictly decreasing terms on this range, so the
/// absolute truncation error is at most the first omitted term, which is enclosed and
/// added symmetrically. Every arithmetic step rounds outward.
pub(crate) fn cos_sin(x: Interval, work: &mut Work) -> NumResult<(Interval, Interval)> {
    if !(x.lo >= 0.0 && x.hi <= 0.8) {
        return Err("internal range reduction left the certified series domain");
    }
    work.step(u64::from(2 * SERIES_TERMS + 2))?;
    let x2 = x.square()?;
    let mut sin_term = x;
    let mut sin = x;
    let mut cos_term = Interval::ONE;
    let mut cos = Interval::ONE;
    for k in 1..=SERIES_TERMS {
        let k = f64::from(k);
        sin_term = sin_term.mul(x2)?.div(Interval::exact((2.0 * k) * (2.0 * k + 1.0)))?;
        cos_term = cos_term.mul(x2)?.div(Interval::exact((2.0 * k - 1.0) * (2.0 * k)))?;
        if k as u32 % 2 == 1 {
            sin = sin.sub(sin_term)?;
            cos = cos.sub(cos_term)?;
        } else {
            sin = sin.add(sin_term)?;
            cos = cos.add(cos_term)?;
        }
    }
    let n = f64::from(SERIES_TERMS + 1);
    let sin_rest = sin_term.mul(x2)?.div(Interval::exact((2.0 * n) * (2.0 * n + 1.0)))?.magnitude();
    let cos_rest = cos_term.mul(x2)?.div(Interval::exact((2.0 * n - 1.0) * (2.0 * n)))?.magnitude();
    let sin = sin.add(Interval { lo: -sin_rest, hi: sin_rest })?;
    let cos = cos.add(Interval { lo: -cos_rest, hi: cos_rest })?;
    let clamp = |i: Interval| Interval { lo: i.lo.max(-1.0), hi: i.hi.min(1.0) };
    Ok((clamp(cos), clamp(sin)))
}

/// Certified unit vector `(cos, sin)` of an exact fixed-point angle `degrees / 10^digits`.
///
/// Exact integer reduction to a quadrant and to at most 45 degrees precedes the series;
/// exact multiples of 90 degrees return exact axis vectors.
pub(crate) fn unit_vector(degrees: i128, digits: u32, work: &mut Work) -> NumResult<(Interval, Interval)> {
    let scale = 10i128.checked_pow(digits).ok_or("angle precision overflow")?;
    let quarter = scale.checked_mul(90).ok_or("angle precision overflow")?;
    let full = quarter.checked_mul(4).ok_or("angle precision overflow")?;
    let t = degrees.rem_euclid(full);
    let quadrant = t / quarter;
    let rho = t % quarter;
    let (c, s) = if rho == 0 {
        (Interval::ONE, Interval::ZERO)
    } else {
        let complement = rho.checked_mul(2).ok_or("angle precision overflow")? > quarter;
        let reduced = if complement { quarter - rho } else { rho };
        let radians = Interval::from_i128(reduced).div(Interval::from_i128(scale))?.mul(Interval::pi())?.div(Interval::exact(180.0))?;
        let (c, s) = cos_sin(radians, work)?;
        if complement { (s, c) } else { (c, s) }
    };
    Ok(match quadrant {
        0 => (c, s),
        1 => (s.neg(), c),
        2 => (c.neg(), s.neg()),
        _ => (s, c.neg()),
    })
}

/// Certified bulge `tan(sweep/4)` of an exact fixed-point sweep in `(0, 90]` degrees.
pub(crate) fn quarter_tangent(sweep: i128, digits: u32, work: &mut Work) -> NumResult<Interval> {
    let scale = 10i128.checked_pow(digits).ok_or("angle precision overflow")?;
    let radians = Interval::from_i128(sweep).div(Interval::from_i128(scale))?.mul(Interval::pi())?.div(Interval::exact(720.0))?;
    let (c, s) = cos_sin(radians, work)?;
    s.div(c)
}

/// Bulge of each half of a circular arc: `b / (1 + sqrt(1 + b^2))`, monotone in `b`.
pub(crate) fn half_bulge(b: Interval) -> NumResult<Interval> {
    let f = |v: f64| -> NumResult<Interval> {
        let x = Interval::exact(v);
        x.div(Interval::ONE.add(Interval::ONE.add(x.square()?)?.sqrt()?)?)
    };
    Ok(Interval { lo: f(b.lo)?.lo, hi: f(b.hi)?.hi })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_grammar_is_exact_and_bounded() {
        assert_eq!(Decimal::parse("12.500").unwrap(), Decimal { neg: false, mant: 125, exp: -1 });
        assert_eq!(Decimal::parse("-0.000").unwrap(), Decimal { neg: false, mant: 0, exp: 0 });
        assert_eq!(Decimal::parse("+1e3").unwrap(), Decimal { neg: false, mant: 1, exp: 3 });
        assert_eq!(Decimal::parse("1.").unwrap(), Decimal { neg: false, mant: 1, exp: 0 });
        assert_eq!(Decimal::parse(".25").unwrap(), Decimal { neg: false, mant: 25, exp: -2 });
        for bad in ["", "-", ".", "NaN", "inf", "0x10", "1e", "1e33", "1e-33", "1.2.3", "1 2", "1d3"] {
            assert!(Decimal::parse(bad).is_err(), "{bad}");
        }
        assert!(Decimal::parse(&"1".repeat(33)).is_err());
        assert!(Decimal::parse(&format!("{}0000", "1".repeat(32))).is_ok());
        assert!(Decimal::parse(&format!("0.{}1", "0".repeat(62))).is_err()); // 65 bytes
        assert_eq!(Decimal::parse("1.000").unwrap().cmp_abs_integer(1), Ordering::Equal);
        assert_eq!(Decimal::parse("-360.0000001").unwrap().cmp_abs_integer(360), Ordering::Greater);
        assert!(Decimal::parse("1.000").unwrap().is_one());
        assert_eq!(Decimal::parse("-12.25").unwrap().fixed(2), Some(-1225));
        assert_eq!(Decimal::parse("12.255").unwrap().fixed(2), None);
    }

    #[test]
    fn intervals_round_outward_and_keep_exact_values_exact() {
        let third = Interval::ONE.div(Interval::exact(3.0)).unwrap();
        assert!(third.lo < 1.0 / 3.0 + 1e-17 && third.hi > 1.0 / 3.0 - 1e-17 && third.lo < third.hi);
        assert_eq!(Interval::exact(1e7).add(Interval::exact(5e6)).unwrap(), Interval::exact(1.5e7));
        assert_eq!(Interval::exact(3.0).mul(Interval::exact(-7.0)).unwrap(), Interval::exact(-21.0));
        let tenth = Interval::decimal(false, 1, -1).unwrap();
        assert!(tenth.lo <= 0.1 && tenth.hi >= 0.1 && tenth.lo < tenth.hi);
        let sum = Interval::exact(1.0).add(Interval::exact(1e-30)).unwrap();
        assert!(sum.lo == 1.0 && sum.hi > 1.0);
        assert_eq!(Interval::exact(16.0).sqrt().unwrap(), Interval::exact(4.0));
        let root2 = Interval::exact(2.0).sqrt().unwrap();
        assert!(root2.lo < std::f64::consts::SQRT_2 + 1e-15 && root2.hi > std::f64::consts::SQRT_2 - 1e-15);
        assert!(root2.mul(root2).unwrap().lo <= 2.0 && root2.mul(root2).unwrap().hi >= 2.0);
        assert!(Interval::exact(1.0).div(Interval { lo: -1.0, hi: 1.0 }).is_err());
        assert!(Interval::exact(f64::MAX).add(Interval::exact(f64::MAX)).is_err());
    }

    #[test]
    fn certified_trig_matches_rational_identities() {
        let mut work = Work { used: 0, limit: 1_000 };
        // 30 and 60 degrees: sin 30 = 1/2 and cos 60 = 1/2 exactly.
        let (c, s) = unit_vector(30, 0, &mut work).unwrap();
        assert!(s.lo <= 0.5 && s.hi >= 0.5 && s.width() < 1e-14, "{s:?} {}", s.width());
        assert!(c.lo <= 3f64.sqrt() / 2.0 + 1e-15 && c.hi >= 3f64.sqrt() / 2.0 - 1e-15);
        let (c, s) = unit_vector(600, 1, &mut work).unwrap();
        assert!(c.lo <= 0.5 && c.hi >= 0.5 && s.lo > 0.86);
        // Exact quadrant boundaries never use the series.
        for (deg, cx, sy) in [(0, 1.0, 0.0), (90, 0.0, 1.0), (180, -1.0, 0.0), (270, 0.0, -1.0), (-90, 0.0, -1.0), (450, 0.0, 1.0)] {
            assert_eq!(unit_vector(deg, 0, &mut work).unwrap(), (Interval::exact(cx), Interval::exact(sy)));
        }
        // Signed quadrant rotations of a 45-degree enclosure contain +-sqrt(1/2).
        let h = std::f64::consts::FRAC_1_SQRT_2;
        for (deg, sx, sy) in [(45, 1.0, 1.0), (135, -1.0, 1.0), (225, -1.0, -1.0), (315, 1.0, -1.0)] {
            let (c, s) = unit_vector(deg, 0, &mut work).unwrap();
            assert!(c.lo <= sx * h && c.hi >= sx * h && s.lo <= sy * h && s.hi >= sy * h, "{deg}");
        }
        // tan(90/4) = sqrt(2) - 1 and tan(60/4) = 2 - sqrt(3).
        let b = quarter_tangent(90, 0, &mut work).unwrap();
        assert!(b.lo <= std::f64::consts::SQRT_2 - 1.0 + 1e-16 && b.hi >= std::f64::consts::SQRT_2 - 1.0 - 1e-16);
        let b = quarter_tangent(60, 0, &mut work).unwrap();
        assert!(b.lo <= 2.0 - 3f64.sqrt() + 1e-16 && b.hi >= 2.0 - 3f64.sqrt() - 1e-16);
        // Halving a semicircle bulge gives tan(pi/8) = sqrt(2) - 1.
        let half = half_bulge(Interval::ONE).unwrap();
        assert!(half.lo <= std::f64::consts::SQRT_2 - 1.0 && half.hi >= std::f64::consts::SQRT_2 - 1.0 - 1e-16);
        let neg = half_bulge(Interval::exact(-1.0)).unwrap();
        assert_eq!(neg, half.neg());
        let mut tiny = Work { used: 0, limit: 10 };
        assert!(unit_vector(1, 0, &mut tiny).is_err());
    }
}
