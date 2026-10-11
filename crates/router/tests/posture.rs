use viamind_geom::{Nm, Point};
use viamind_router::{CornerMode, Posture, plan_pad_path};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
#[test]
fn explicit_postures_have_independent_expected_bends_in_all_quadrants() {
    for (sx, sy) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
        let a = p(0, 0);
        let b = p(sx * 600, sy * 400);
        for (mode, posture, bend) in [
            (CornerMode::Cardinal, Posture::Primary, p(sx * 600, 0)),
            (CornerMode::Cardinal, Posture::Alternate, p(0, sy * 400)),
            (CornerMode::Octilinear, Posture::Primary, p(sx * 200, 0)),
            (CornerMode::Octilinear, Posture::Alternate, p(sx * 400, sy * 400)),
        ] {
            let plan = plan_pad_path(a, b, &[], Nm(100), mode, posture).unwrap();
            assert_eq!(plan.points, [a, bend, b]);
            assert_eq!(plan.endpoint_stub_segments, 0);
        }
        // Y-major counterparts choose the remaining Y-axis straight segment.
        let b = p(sx * 400, sy * 600);
        assert_eq!(plan_pad_path(a, b, &[], Nm(100), CornerMode::Octilinear, Posture::Primary).unwrap().points, [a, p(0, sy * 200), b]);
        assert_eq!(plan_pad_path(a, b, &[], Nm(100), CornerMode::Octilinear, Posture::Alternate).unwrap().points, [a, p(sx * 400, sy * 400), b]);
    }
}
#[test]
fn exact_offgrid_odd_signed_stubs_and_grid_constraints_are_preserved() {
    let start = p(-2, 1);
    let target = p(22, -14);
    for mode in [CornerMode::Cardinal, CornerMode::Octilinear] {
        for posture in [Posture::Primary, Posture::Alternate] {
            let plan = plan_pad_path(start, target, &[p(9, 9)], Nm(3), mode, posture).unwrap();
            assert_eq!((plan.points[0], *plan.points.last().unwrap()), (start, target));
            assert_eq!(plan.endpoint_stub_segments, 2);
            assert_eq!(plan.points[1], p(-3, 0));
            assert_eq!(plan.points[plan.points.len() - 2], p(21, -15));
            assert!(plan.points.contains(&p(9, 9)));
            for pair in plan.points[1..plan.points.len() - 1].windows(2) {
                assert_eq!((pair[0].x.0 % 3, pair[0].y.0 % 3, pair[1].x.0 % 3, pair[1].y.0 % 3), (0, 0, 0, 0));
                let dx = (pair[1].x.0 - pair[0].x.0).abs();
                let dy = (pair[1].y.0 - pair[0].y.0).abs();
                assert!(dx == 0 || dy == 0 || (mode == CornerMode::Octilinear && dx == dy));
            }
        }
    }
}
#[test]
fn single_segments_deduplicate_bends_but_preserve_ordered_revisits() {
    for posture in [Posture::Primary, Posture::Alternate] {
        for target in [p(500, 0), p(0, -500), p(500, 500), p(-500, 500)] {
            assert_eq!(plan_pad_path(p(0, 0), target, &[], Nm(100), CornerMode::Octilinear, posture).unwrap().points, [p(0, 0), target]);
        }
    }
    let plan = plan_pad_path(p(0, 0), p(200, 0), &[p(100, 0), p(100, 0), p(0, 0)], Nm(100), CornerMode::Cardinal, Posture::Primary).unwrap();
    assert_eq!(plan.points, [p(0, 0), p(100, 0), p(0, 0), p(200, 0)]);
}
#[test]
fn invalid_bounded_inputs_and_impossible_model_anchor_reject_without_overflow() {
    let attempt = |a, b, w: &[Point], grid| plan_pad_path(a, b, w, Nm(grid), CornerMode::Octilinear, Posture::Primary);
    for (a, b, w, grid) in [
        (p(0, 0), p(100, 100), vec![], 0),
        (p(0, 0), p(100, 100), vec![], -1),
        (p(0, 0), p(100, 100), vec![], 10_000_000_001),
        (p(i64::MIN, 0), p(i64::MAX, 0), vec![], 1),
        (p(0, 0), p(100, 100), vec![p(1, 1)], 100),
        (p(0, 0), p(100, 100), vec![p(0, 0); 511], 100),
        (p(0, 0), p(10_000_000_000, 0), vec![], 6_000_000_000),
        (p(0, 0), p(0, 0), vec![], 1),
    ] {
        assert!(attempt(a, b, &w, grid).is_err());
    }
    let plan = attempt(p(-10_000_000_000, -10_000_000_000), p(10_000_000_000, 10_000_000_000), &[], 1).unwrap();
    assert_eq!(plan.points.len(), 2);
    let waypoints: Vec<_> = (0..510).map(|i| if i % 2 == 0 { p(600, 400) } else { p(0, 0) }).collect();
    let plan = attempt(p(-1, -1), p(601, 401), &waypoints, 100).unwrap();
    assert!(plan.points.len() <= 1025);
    assert_eq!(plan.endpoint_stub_segments, 2);
}
