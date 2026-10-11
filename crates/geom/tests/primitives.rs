//! Property tests for geometry primitives: intersections, distances and offsets.

use proptest::prelude::*;
use viamind_geom::{Angle, Arc, Capsule, Circle, Nm, Point, Polygon, Rect, Segment, SegmentIntersection, Shape, Vector};

/// About ±1 m, larger than any board.
const BOARD: i64 = 1_000_000_000;
/// About ±10 m: products of coordinate differences overflow `i64` here.
const HUGE: i64 = 10_000_000_000;

fn pt(range: i64) -> impl Strategy<Value = Point> + Clone {
    (-range..=range, -range..=range).prop_map(|(x, y)| Point::new(Nm(x), Nm(y)))
}

/// Points on a coarse grid, so collinear and touching cases come up often.
fn grid_pt() -> impl Strategy<Value = Point> + Clone {
    (-8i64..=8, -8i64..=8).prop_map(|(x, y)| Point::new(Nm(x * 1000), Nm(y * 1000)))
}

fn seg(p: impl Strategy<Value = Point> + Clone) -> impl Strategy<Value = Segment> {
    (p.clone(), p).prop_map(|(a, b)| Segment::new(a, b))
}

fn rect() -> impl Strategy<Value = Rect> {
    (pt(BOARD), pt(BOARD)).prop_map(|(a, b)| Rect::new(a, b))
}

fn len(max: i64) -> impl Strategy<Value = Nm> {
    (0..=max).prop_map(Nm)
}

fn shape() -> impl Strategy<Value = Shape> {
    let r = 10_000_000;
    prop_oneof![
        (pt(r), len(r / 10)).prop_map(|(c, rad)| Shape::from(Circle::new(c, rad))),
        (seg(pt(r)), len(r / 10)).prop_map(|(s, w)| Shape::from(Capsule::new(s, w))),
        (pt(r), pt(r)).prop_map(|(a, b)| Shape::from(Rect::new(a, b))),
        (pt(r), len(r / 10), 3usize..12).prop_map(|(c, rad, n)| Shape::from(regular_polygon(c, rad.0.max(10), n))),
    ]
}

/// A convex polygon with `n` vertices on a circle (rounded to the grid).
fn regular_polygon(c: Point, r: i64, n: usize) -> Polygon {
    let circle = Arc::new(c, Nm(r), Angle::ZERO, Angle::FULL);
    Polygon::new((0..n).map(|i| circle.point_at(Angle(Angle::FULL.0 / n as i64 * i as i64))).collect())
}

/// Turns a point a quarter turn about the origin; exact on the integer grid.
fn rot90(p: Point) -> Point {
    Point::new(-p.y, p.x)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

proptest! {
    // ---- Rect ----

    #[test]
    fn rect_union_contains_both(a in rect(), b in rect()) {
        let u = a.union(&b);
        prop_assert!(u.contains_rect(&a) && u.contains_rect(&b));
    }

    #[test]
    fn rect_intersects_matches_intersection(a in rect(), b in rect(), p in pt(BOARD)) {
        prop_assert_eq!(a.intersects(&b), b.intersects(&a));
        prop_assert_eq!(a.intersects(&b), a.intersection(&b).is_some());
        if let Some(i) = a.intersection(&b) {
            prop_assert!(a.contains_rect(&i) && b.contains_rect(&i));
        }
        prop_assert_eq!(a.contains(p) && b.contains(p), a.intersection(&b).is_some_and(|i| i.contains(p)));
    }

    #[test]
    fn rect_inflate_is_an_offset(r in rect(), d in len(BOARD / 10), p in pt(BOARD)) {
        let big = r.inflate(d);
        prop_assert!(big.contains_rect(&r));
        // Inflating by d holds every point within d along each axis.
        let dx = (r.min.x.0 - p.x.0).max(p.x.0 - r.max.x.0).max(0);
        let dy = (r.min.y.0 - p.y.0).max(p.y.0 - r.max.y.0).max(0);
        prop_assert_eq!(big.contains(p), dx <= d.0 && dy <= d.0);
        prop_assert!(r.inflate(-d).inflate(d).contains_rect(&r.inflate(-d)));
        prop_assert!(r.contains_rect(&r.inflate(-d)));
    }

    // ---- Segments ----

    #[test]
    fn segment_intersects_is_symmetric_and_invariant(a in seg(grid_pt()), b in seg(grid_pt()), v in pt(BOARD)) {
        let hit = a.intersects(&b);
        prop_assert_eq!(hit, b.intersects(&a));
        prop_assert_eq!(hit, a.reversed().intersects(&b));
        let v = v - Point::ORIGIN;
        prop_assert_eq!(hit, a.translate(v).intersects(&b.translate(v)));
        let r = |s: Segment| Segment::new(rot90(s.a), rot90(s.b));
        prop_assert_eq!(hit, r(a).intersects(&r(b)));
        prop_assert_eq!(hit, a.distance_to_segment(&b) == 0.0);
        prop_assert_eq!(hit, a.intersection(&b) != SegmentIntersection::None);
    }

    #[test]
    fn segment_intersection_lies_on_both(a in seg(pt(BOARD)), b in seg(pt(BOARD))) {
        match a.intersection(&b) {
            SegmentIntersection::None => prop_assert!(a.distance_to_segment(&b) > 0.0),
            SegmentIntersection::Point(p) => {
                prop_assert!(a.distance_to_point(p) <= 1.0, "{}", a.distance_to_point(p));
                prop_assert!(b.distance_to_point(p) <= 1.0, "{}", b.distance_to_point(p));
            }
            SegmentIntersection::Overlap(o) => {
                for p in [o.a, o.b] {
                    prop_assert!(a.distance_to_point(p) == 0.0 && b.distance_to_point(p) == 0.0);
                }
            }
        }
    }

    #[test]
    fn segment_overlap_on_grid(a in seg(grid_pt()), b in seg(grid_pt())) {
        if let SegmentIntersection::Overlap(o) = a.intersection(&b) {
            prop_assert!(o.a != o.b);
            prop_assert!(a.distance_to_point(o.a) == 0.0 && b.distance_to_point(o.b) == 0.0);
        }
    }

    #[test]
    fn segments_at_ten_metres_stay_exact(a in seg(pt(HUGE)), b in seg(pt(HUGE)), v in pt(HUGE)) {
        let v = v - Point::ORIGIN;
        prop_assert_eq!(a.intersects(&b), a.translate(v).intersects(&b.translate(v)));
        prop_assert_eq!(a.intersects(&b), b.intersects(&a));
    }

    #[test]
    fn point_segment_distance_is_the_minimum(s in seg(pt(BOARD)), p in pt(BOARD), t in 0.0f64..=1.0) {
        let d = s.distance_to_point(p);
        prop_assert!(d <= s.a.distance(p) && d <= s.b.distance(p));
        // Any sampled point on the segment is no closer (within a nanometre of rounding).
        let v = s.vector();
        let q = s.a + Vector::new(Nm((v.x.0 as f64 * t).round() as i64), Nm((v.y.0 as f64 * t).round() as i64));
        prop_assert!(d <= q.distance(p) + 1.0);
        // The closest point realises it.
        prop_assert!((s.closest_point(p).distance(p) - d).abs() <= 1.0);
    }

    #[test]
    fn segment_distance_is_symmetric_and_bounded(a in seg(pt(BOARD)), b in seg(pt(BOARD))) {
        let d = a.distance_to_segment(&b);
        prop_assert_eq!(d, b.distance_to_segment(&a));
        prop_assert!(d >= 0.0);
        prop_assert!(d <= a.a.distance(b.a));
    }

    // ---- Polygons ----

    #[test]
    fn rect_polygon_matches_rect(r in rect(), p in pt(BOARD)) {
        let poly = Polygon::from_rect(&r);
        prop_assert_eq!(poly.signed_area_x2(), 2 * i128::from(r.width().0) * i128::from(r.height().0));
        prop_assert_eq!(poly.contains(p), r.contains(p));
        prop_assert_eq!(poly.bbox(), Some(r));
        prop_assert!(close(poly.distance_to_point(p), r.distance_to_point(p)));
    }

    #[test]
    fn polygon_area_and_containment_survive_moves(
        c in pt(BOARD / 2), r in 10i64..BOARD / 4, n in 3usize..40, v in pt(BOARD / 2), p in pt(BOARD),
    ) {
        let poly = regular_polygon(c, r, n);
        let v = v - Point::ORIGIN;
        let moved = poly.translate(v);
        prop_assert_eq!(poly.signed_area_x2(), moved.signed_area_x2());
        prop_assert_eq!(poly.contains(p), moved.contains(p + v));
        let rev = Polygon::new(poly.points.iter().rev().copied().collect());
        prop_assert_eq!(rev.signed_area_x2(), -poly.signed_area_x2());
        prop_assert_eq!(rev.contains(p), poly.contains(p));
        for &q in &poly.points {
            prop_assert!(poly.contains(q));
        }
        prop_assert!(poly.contains(c));
    }

    // ---- Arcs ----

    #[test]
    fn arc_points_lie_on_circle_and_in_bbox(
        c in pt(BOARD), r in 1i64..BOARD, start in -720i64..720, sweep in -400i64..400, tol in 1i64..100_000,
    ) {
        let arc = Arc::new(c, Nm(r), Angle::from_degrees(start as f64), Angle::from_degrees(sweep as f64));
        let bbox = arc.bbox();
        let pts = arc.flatten(Nm(tol));
        prop_assert_eq!(pts.first(), Some(&arc.start_point()));
        prop_assert_eq!(pts.last(), Some(&arc.end_point()));
        for w in pts.windows(2) {
            for q in w {
                prop_assert!((q.distance(c) - r as f64).abs() <= 1.0);
                prop_assert!(bbox.contains(*q));
                prop_assert!(arc.distance_to_point(*q) <= 1.0);
            }
            // Chord midpoint stays within the tolerance of the arc.
            let m = Point::new(Nm((w[0].x.0 + w[1].x.0) / 2), Nm((w[0].y.0 + w[1].y.0) / 2));
            prop_assert!(r as f64 - m.distance(c) <= tol as f64 + 2.0);
        }
        prop_assert!(arc.contains_angle(arc.start) && arc.contains_angle(arc.end_angle()));
        prop_assert_eq!(arc.reversed().bbox(), bbox);
        prop_assert!(close(arc.reversed().length(), arc.length()));
    }

    #[test]
    fn arc_from_three_points_round_trips(c in pt(BOARD), r in 1_000i64..BOARD, start in 0i64..360, sweep in 1i64..359, ccw: bool) {
        let sweep = if ccw { sweep } else { -sweep };
        let arc = Arc::new(c, Nm(r), Angle::from_degrees(start as f64), Angle::from_degrees(sweep as f64));
        let (a, m, b) = (arc.start_point(), arc.mid_point(), arc.end_point());
        // Very flat arcs can round onto a straight line.
        let back = Arc::from_three_points(a, m, b);
        prop_assume!(back.is_some());
        let back = back.unwrap();
        // The fitted arc passes through its inputs, in the same direction.
        prop_assert!(back.start_point().distance(a) <= 3.0, "start off by {}", back.start_point().distance(a));
        prop_assert!(back.end_point().distance(b) <= 3.0, "end off by {}", back.end_point().distance(b));
        prop_assert!(back.distance_to_point(m) <= 3.0, "mid off by {}", back.distance_to_point(m));
        prop_assert_eq!(back.sweep.0.signum(), arc.sweep.0.signum());
        // It recovers the original circle up to the grid rounding of the inputs, which the fit
        // amplifies by about radius / sagitta for shallow arcs and radius / gap for nearly full ones.
        if (20..=300).contains(&sweep.abs()) {
            let sagitta = 1.0 - (sweep.abs() as f64 / 2.0).to_radians().cos();
            let tol = 4.0 + 4.0 / sagitta.min(1.0);
            prop_assert!(back.center.distance(c) <= tol, "centre off by {}", back.center.distance(c));
            prop_assert!(((back.radius.0 - r).abs() as f64) <= tol);
        }
    }

    // ---- Shapes, clearances and offsets ----

    #[test]
    fn shape_distance_is_symmetric_and_translation_invariant(a in shape(), b in shape(), v in pt(BOARD)) {
        let d = a.distance(&b);
        prop_assert!(d >= 0.0);
        prop_assert!(close(d, b.distance(&a)));
        let v = v - Point::ORIGIN;
        prop_assert!(close(d, a.translate(v).distance(&b.translate(v))));
        prop_assert_eq!(a.intersects(&b), b.intersects(&a));
        // Shapes with disjoint boxes cannot touch.
        if !a.bbox().intersects(&b.bbox()) {
            prop_assert!(d > 0.0);
        }
    }

    #[test]
    fn shape_contains_its_reference_points(s in shape()) {
        let probe = match &s {
            Shape::Circle(c) => vec![c.center],
            Shape::Capsule(c) => vec![c.segment.a, c.segment.b],
            Shape::Rect(r) => r.corners().to_vec(),
            Shape::Polygon(p) => p.points.clone(),
        };
        for p in probe {
            prop_assert!(s.contains(p));
            prop_assert!(s.bbox().contains(p));
        }
    }

    #[test]
    fn circle_clearance_is_centre_distance_minus_radii(a in pt(BOARD), b in pt(BOARD), ra in len(BOARD / 10), rb in len(BOARD / 10)) {
        let d = Shape::from(Circle::new(a, ra)).distance(&Shape::from(Circle::new(b, rb)));
        prop_assert!(close(d, (a.distance(b) - ra.0 as f64 - rb.0 as f64).max(0.0)));
    }

    #[test]
    fn inflating_reduces_clearance_by_the_offset(
        a in seg(pt(BOARD)), b in seg(pt(BOARD)), wa in len(BOARD / 100), wb in len(BOARD / 100), d in len(BOARD / 100),
    ) {
        let (ca, cb) = (Capsule::new(a, wa), Capsule::new(b, wb));
        let before = ca.clearance(&cb);
        let after = ca.inflate(d).clearance(&cb);
        prop_assert!(close(after, (before - d.0 as f64).max(0.0)) || before == 0.0);
        prop_assert!(close(before, Shape::from(ca).distance(&Shape::from(cb))));
        let circ = Circle::new(a.a, wa);
        let p = b.b;
        prop_assert!(close(
            Shape::from(circ.inflate(d)).distance_to_point(p),
            (Shape::from(circ).distance_to_point(p) - d.0 as f64).max(0.0),
        ));
    }

    #[test]
    fn capsule_bbox_holds_its_outline(s in seg(pt(BOARD)), w in len(BOARD / 10), t in 0.0f64..std::f64::consts::TAU) {
        let c = Capsule::new(s, w);
        let (sin, cos) = t.sin_cos();
        let half = w.0 as f64 / 2.0;
        for end in [s.a, s.b] {
            let q = end + Vector::new(Nm((half * cos).trunc() as i64), Nm((half * sin).trunc() as i64));
            prop_assert!(c.bbox().contains(q));
            prop_assert!(Shape::from(c).contains(q));
        }
    }
}
