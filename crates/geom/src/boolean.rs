//! Polygon booleans and offsets on the nanometre grid.
//!
//! A thin layer over the integer API of [`i_overlay`] (see
//! `docs/decisions/0002-polygon-booleans.md`). Inputs are sets of
//! [`Polygon`] rings combined with the non-zero winding rule, so a clockwise
//! ring inside a counter-clockwise one is a hole. Outputs are [`Region`]s whose
//! coordinates stay exactly on the nanometre grid.
//!
//! Internally coordinates are shifted to the centre of the inputs' bounding box
//! and run on the fast `i32` engine when everything fits in about ±1 m around
//! that centre, otherwise on the `i64` engine.

use std::f64::consts::TAU;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::integer::OverlayInt;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::angle::Angle as TurnAngle;
use i_overlay::i_float::int::point::IntPoint;
use i_overlay::mesh::int::arc::ArcOptions;
use i_overlay::mesh::int::outline::offset::IntOutlineOffset;
use i_overlay::mesh::int::style::{IntLineJoin, IntOutlineStyle};

use crate::point::{Point, Vector};
use crate::polygon::Polygon;
use crate::rect::Rect;
use crate::units::Nm;

/// A filled region: one outer boundary (counter-clockwise) and its holes (clockwise).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region {
    /// The outer boundary, counter-clockwise.
    pub outer: Polygon,
    /// Holes inside `outer`, clockwise.
    pub holes: Vec<Polygon>,
}

impl Region {
    /// The outer boundary followed by the holes; feed these back into another operation.
    pub fn rings(&self) -> impl Iterator<Item = &Polygon> {
        std::iter::once(&self.outer).chain(&self.holes)
    }

    /// Area in nm², holes excluded.
    pub fn area(&self) -> f64 {
        self.rings().map(Polygon::signed_area_x2).sum::<i128>() as f64 / 2.0
    }
}

/// The union of all `polygons`.
pub fn union<'a>(polygons: impl IntoIterator<Item = &'a Polygon>) -> Vec<Region> {
    run(&collect(polygons), &[], 0, Op::Union)
}

/// The parts of `subject` that are also in `clip`.
pub fn intersection<'a, 'b>(subject: impl IntoIterator<Item = &'a Polygon>, clip: impl IntoIterator<Item = &'b Polygon>) -> Vec<Region> {
    run(&collect(subject), &collect(clip), 0, Op::Rule(OverlayRule::Intersect))
}

/// The parts of `subject` that are not in `clip` (for example a pour minus its obstacles).
pub fn difference<'a, 'b>(subject: impl IntoIterator<Item = &'a Polygon>, clip: impl IntoIterator<Item = &'b Polygon>) -> Vec<Region> {
    run(&collect(subject), &collect(clip), 0, Op::Rule(OverlayRule::Difference))
}

/// Grows the union of `polygons` by `distance` (shrinks if negative), with round corners.
///
/// Corner arcs are flattened so that they stay within `tolerance` of the true
/// arc where the engine's finest step (1/1024 turn) allows. Prefer growing
/// circles and capsules analytically ([`crate::Circle::inflate`],
/// [`crate::Capsule::inflate`]) before flattening them: offsetting an
/// already-flattened curve multiplies its vertex count.
pub fn offset<'a>(polygons: impl IntoIterator<Item = &'a Polygon>, distance: Nm, tolerance: Nm) -> Vec<Region> {
    if distance == Nm::ZERO {
        return union(polygons);
    }
    let d = distance.0.unsigned_abs() as f64;
    let step = 2.0 * (1.0 - (tolerance.0.max(1) as f64 / d).min(1.0)).acos();
    run(&collect(polygons), &[], distance.0, Op::Offset { step })
}

#[derive(Clone, Copy)]
enum Op {
    Union,
    Rule(OverlayRule),
    Offset { step: f64 },
}

fn collect<'a>(polygons: impl IntoIterator<Item = &'a Polygon>) -> Vec<&'a Polygon> {
    polygons.into_iter().filter(|p| p.points.len() >= 3).collect()
}

/// Largest coordinate magnitude that is safe for an engine with `bits` bits.
const fn safe_range(bits: u32) -> i64 {
    (1i64 << (bits - 2)) - 1
}

fn run(subject: &[&Polygon], clip: &[&Polygon], grow: i64, op: Op) -> Vec<Region> {
    let Some(bbox) = Rect::from_points(subject.iter().chain(clip).flat_map(|p| p.points.iter().copied())) else {
        return Vec::new();
    };
    let center = bbox.center();
    // Half the extent plus room for the offset and its joins.
    let reach = (bbox.width().0 / 2 + 1).max(bbox.height().0 / 2 + 1).saturating_add(grow.saturating_abs().saturating_mul(2));
    let origin = center - Point::ORIGIN;
    if reach <= safe_range(32) {
        run_with::<i32>(subject, clip, grow, op, origin)
    } else {
        assert!(reach <= safe_range(64), "geometry too large for the boolean engine");
        run_with::<i64>(subject, clip, grow, op, origin)
    }
}

/// An `i_overlay` engine integer that converts losslessly to and from `i64` within its safe range.
trait Engine: OverlayInt + TryFrom<i64> + Into<i64> {}
impl Engine for i32 {}
impl Engine for i64 {}

fn run_with<I: Engine>(subject: &[&Polygon], clip: &[&Polygon], grow: i64, op: Op, origin: Vector) -> Vec<Region> {
    // `run` checked the range, so these conversions cannot fail.
    let to_engine = |v: i64| I::try_from(v).unwrap_or_else(|_| unreachable!("coordinate checked against the engine range"));
    let to_int = |polys: &[&Polygon]| -> Vec<Vec<IntPoint<I>>> {
        polys
            .iter()
            .map(|p| p.points.iter().map(|&q| IntPoint::new(to_engine((q.x - origin.x).0), to_engine((q.y - origin.y).0))).collect())
            .collect()
    };
    let subj = to_int(subject);
    let shapes = match op {
        Op::Union => Overlay::from_subj(&subj).overlay(OverlayRule::Subject, FillRule::NonZero),
        Op::Rule(rule) => Overlay::from_subj_and_clip(&subj, &to_int(clip)).overlay(rule, FillRule::NonZero),
        Op::Offset { step } => {
            let bits = (step / TAU * 2f64.powi(32)).clamp(f64::from(ArcOptions::MIN_STEP.bits()), f64::from(ArcOptions::MAX_STEP.bits()));
            let join = IntLineJoin::Round(ArcOptions { max_step: TurnAngle::from_bits(bits as u32), ..ArcOptions::default() });
            // Offsetting expects outer rings counter-clockwise; normalise them through a union first.
            let unioned = Overlay::from_subj(&subj).overlay(OverlayRule::Subject, FillRule::NonZero);
            let rings: Vec<Vec<IntPoint<I>>> = unioned.into_iter().flatten().collect();
            rings.outline(&IntOutlineStyle::new(to_engine(grow)).line_join(join)).unwrap_or_default()
        }
    };
    let from_int = |c: Vec<IntPoint<I>>| Polygon::new(c.into_iter().map(|p| Point::new(Nm(p.x.into()), Nm(p.y.into())) + origin).collect());
    shapes
        .into_iter()
        .filter_map(|shape| {
            let mut rings = shape.into_iter().map(from_int);
            let outer = rings.next()?;
            Some(Region { outer, holes: rings.collect() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: i64, y: i64, size: i64) -> Polygon {
        Polygon::from_rect(&Rect::new(Point::new(Nm(x), Nm(y)), Point::new(Nm(x + size), Nm(y + size))))
    }

    #[test]
    fn union_and_difference() {
        let a = square(0, 0, 10);
        let b = square(5, 5, 10);
        let u = union([&a, &b]);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].area(), 175.0);
        assert!(u[0].outer.is_ccw());

        let d = difference([&a], [&b]);
        assert_eq!(d.iter().map(Region::area).sum::<f64>(), 75.0);

        let i = intersection([&a], [&b]);
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].area(), 25.0);

        // A hole: big square minus a small one inside it.
        let ring = difference([&square(0, 0, 30)], [&square(10, 10, 10)]);
        assert_eq!(ring.len(), 1);
        assert_eq!(ring[0].holes.len(), 1);
        assert!(!ring[0].holes[0].is_ccw());
        assert_eq!(ring[0].area(), 800.0);
        // Feeding the rings back in reproduces the region.
        assert_eq!(union(ring[0].rings()).iter().map(Region::area).sum::<f64>(), 800.0);
        assert!(union([]).is_empty());
    }

    #[test]
    fn large_coordinates_use_wide_engine() {
        let far = 3_000_000_000_000; // 3 km: past the i32 engine's reach around any centre.
        let u = union([&square(-far, 0, 1000), &square(far, 0, 1000)]);
        assert_eq!(u.len(), 2);
        assert_eq!(u.iter().map(Region::area).sum::<f64>(), 2_000_000.0);
        // Far from the origin but compact: shifted onto the i32 engine, still exact.
        let d = difference([&square(far, far, 1000)], [&square(far + 500, far, 1000)]);
        assert_eq!(d[0].outer.bbox(), Some(Rect::new(Point::new(Nm(far), Nm(far)), Point::new(Nm(far + 500), Nm(far + 1000)))));
    }

    #[test]
    fn offset_grows_and_shrinks() {
        let mm = 1_000_000;
        let s = square(0, 0, 10 * mm);
        let grown = offset([&s], Nm(mm), Nm(1000));
        assert_eq!(grown.len(), 1);
        // A 12 × 12 mm square with its corners rounded to radius 1 mm: 144 - (4 - π) mm².
        let exact = (144.0 - (4.0 - std::f64::consts::PI)) * 1e12;
        assert!((grown[0].area() - exact).abs() / exact < 1e-4, "{}", grown[0].area());
        let shrunk = offset([&s], Nm(-mm), Nm(1000));
        assert_eq!(shrunk[0].area(), 64.0 * 1e12);
        assert!(offset([&s], Nm(-6 * mm), Nm(1000)).is_empty());
    }
}
