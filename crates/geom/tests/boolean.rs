//! Property tests for polygon booleans and offsets.

use proptest::prelude::*;
use viamind_geom::boolean::{difference, intersection, offset, union};
use viamind_geom::{Angle, Arc, Nm, Point, Polygon, Rect, Region};

const SPAN: i64 = 50_000_000;

fn pt() -> impl Strategy<Value = Point> {
    (-SPAN..=SPAN, -SPAN..=SPAN).prop_map(|(x, y)| Point::new(Nm(x), Nm(y)))
}

fn rect() -> impl Strategy<Value = Rect> {
    (pt(), 1i64..SPAN, 1i64..SPAN).prop_map(|(p, w, h)| Rect::new(p, Point::new(p.x + Nm(w), p.y + Nm(h))))
}

/// A convex polygon with vertices on a circle.
fn convex() -> impl Strategy<Value = Polygon> {
    (pt(), 1_000i64..SPAN, 3usize..24, 0i64..360).prop_map(|(c, r, n, rot)| {
        let circle = Arc::new(c, Nm(r), Angle::from_degrees(rot as f64), Angle::FULL);
        Polygon::new((0..n).map(|i| circle.point_at(circle.start + Angle(Angle::FULL.0 / n as i64 * i as i64))).collect())
    })
}

fn area(regions: &[Region]) -> f64 {
    regions.iter().map(Region::area).sum()
}

/// Whether `p` is strictly inside the regions (not within `margin` of any boundary).
fn inside(regions: &[Region], p: Point, margin: f64) -> Option<bool> {
    let near_edge = regions.iter().flat_map(Region::rings).any(|r| r.edges().any(|e| e.distance_to_point(p) <= margin));
    if near_edge {
        return None;
    }
    Some(regions.iter().any(|r| r.outer.contains(p) && !r.holes.iter().any(|h| h.contains(p))))
}

proptest! {
    #[test]
    fn rect_booleans_are_exact(a in rect(), b in rect()) {
        let (pa, pb) = (Polygon::from_rect(&a), Polygon::from_rect(&b));
        let overlap = a.intersection(&b).map_or(0.0, |i| i.width().0 as f64 * i.height().0 as f64);
        let (area_a, area_b) = (pa.area(), pb.area());
        prop_assert_eq!(area(&intersection([&pa], [&pb])), overlap);
        prop_assert_eq!(area(&union([&pa, &pb])), area_a + area_b - overlap);
        prop_assert_eq!(area(&difference([&pa], [&pb])), area_a - overlap);
    }

    #[test]
    fn boolean_membership_matches_point_tests(a in convex(), b in convex(), probes in prop::collection::vec(pt(), 32)) {
        let u = union([&a, &b]);
        let i = intersection([&a], [&b]);
        let d = difference([&a], [&b]);
        for p in probes {
            let (in_a, in_b) = (a.contains(p), b.contains(p));
            // Skip probes on or next to an input boundary (outputs snap crossings to the grid).
            if a.edges().chain(b.edges()).any(|e| e.distance_to_point(p) <= 2.0) {
                continue;
            }
            if let Some(v) = inside(&u, p, 2.0) { prop_assert_eq!(v, in_a || in_b); }
            if let Some(v) = inside(&i, p, 2.0) { prop_assert_eq!(v, in_a && in_b); }
            if let Some(v) = inside(&d, p, 2.0) { prop_assert_eq!(v, in_a && !in_b); }
        }
        // Inclusion–exclusion, up to the snapping of crossing points to the grid.
        let tol = 1e-6 * (a.area() + b.area()) + 1e9;
        prop_assert!((area(&u) + area(&i) - a.area() - b.area()).abs() <= tol);
    }

    #[test]
    fn offset_contains_input_and_respects_distance(a in convex(), d in 1_000i64..1_000_000, probes in prop::collection::vec(pt(), 16)) {
        let grown = offset([&a], Nm(d), Nm(100));
        for &v in &a.points {
            prop_assert_eq!(inside(&grown, v, 0.0), Some(true));
        }
        for p in probes {
            let dist = a.distance_to_point(p);
            // Points clearly within the offset distance are inside; clearly beyond it, outside.
            if dist < d as f64 - 200.0 {
                prop_assert_ne!(inside(&grown, p, 0.0), Some(false));
            } else if dist > d as f64 + 200.0 {
                prop_assert_ne!(inside(&grown, p, 0.0), Some(true));
            }
        }
        prop_assert!(area(&grown) >= a.area());
    }
}
