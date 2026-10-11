use viamind_geom::{Nm, Point, Rect};
use viamind_router::plan_walkaround;

fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn r(x: i64, y: i64, xx: i64, yy: i64) -> Rect {
    Rect::new(p(x, y), p(xx, yy))
}
fn length(points: &[Point]) -> i64 {
    points.windows(2).map(|pair| (pair[0].x.0 - pair[1].x.0).abs() + (pair[0].y.0 - pair[1].y.0).abs()).sum()
}
#[test]
fn blocked_direct_path_has_deterministic_shortest_grid_detour() {
    let domain = r(0, 0, 40, 20);
    let obstacles = [r(18, 8, 22, 12)];
    let plan = plan_walkaround(p(10, 10), p(30, 10), Nm(1), domain, &obstacles).unwrap();
    assert_eq!(plan, plan_walkaround(p(10, 10), p(30, 10), Nm(1), domain, &obstacles).unwrap());
    // Independently: 20 horizontal units + two3-unit escapes from closed y8..12.
    assert_eq!(length(&plan.path.points), 26);
    assert!(plan.path.points.iter().any(|point| point.y.0 == 7));
    for pair in plan.path.points.windows(2) {
        assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
        assert!(domain.contains_rect(&Rect::new(pair[0], pair[1])));
        assert!(!obstacles[0].intersects(&Rect::new(pair[0], pair[1])));
    }
    // Closing the upper corridor forces the longer lower escape.
    let plan = plan_walkaround(p(10, 10), p(30, 10), Nm(1), domain, &[r(18, 0, 22, 12)]).unwrap();
    assert_eq!(length(&plan.path.points), 26);
    assert!(plan.path.points.iter().any(|point| point.y.0 == 13));
    assert!(plan_walkaround(p(10, 10), p(30, 10), Nm(1), domain, &[r(18, 0, 22, 20)]).unwrap_err().contains("no static cardinal path"));
}
#[test]
fn signed_odd_grid_and_exact_endpoint_stubs_are_preserved() {
    let plan = plan_walkaround(p(-14, -14), p(14, -14), Nm(3), r(-30, -30, 30, 30), &[r(-2, -17, 2, -13)]).unwrap();
    assert_eq!(plan.path.points.first(), Some(&p(-14, -14)));
    assert_eq!(plan.path.points.last(), Some(&p(14, -14)));
    assert_eq!(plan.path.points[1], p(-15, -15));
    assert_eq!(plan.path.points[plan.path.points.len() - 2], p(15, -15));
    assert_eq!(plan.path.endpoint_stub_segments, 2);
    assert!(plan.path.points.iter().any(|point| point.y.0 == -18));
    for pair in plan.path.points[1..plan.path.points.len() - 1].windows(2) {
        assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
        assert!(pair[0].x.0 % 3 == 0 && pair[0].y.0 % 3 == 0);
        assert!(!r(-2, -17, 2, -13).intersects(&Rect::new(pair[0], pair[1])));
    }
    // Closed-box contact and conservative stub bounds refuse unsafe escape.
    assert!(plan_walkaround(p(1, 1), p(20, 20), Nm(3), r(0, 0, 30, 30), &[r(0, 0, 0, 0)]).unwrap_err().contains("stub"));
}
#[test]
fn zero_obstacles_coincident_anchors_and_domain_refusals_are_explicit() {
    let direct = plan_walkaround(p(0, 0), p(30, 0), Nm(3), r(-30, -30, 30, 30), &[]).unwrap();
    assert_eq!(direct.path.points, [p(0, 0), p(30, 0)]);
    assert!(direct.waypoints.is_empty());
    let short = plan_walkaround(p(1, 1), p(-1, -1), Nm(3), r(-30, -30, 30, 30), &[]).unwrap();
    assert_eq!(short.path.points, [p(1, 1), p(0, 0), p(-1, -1)]);
    for result in [
        plan_walkaround(p(0, 0), p(30, 0), Nm(0), r(-30, -30, 30, 30), &[]),
        plan_walkaround(p(0, 0), p(30, 0), Nm(1), r(0, 0, 0, 30), &[]),
        plan_walkaround(p(0, 0), p(30, 0), Nm(1), r(1, 1, 30, 30), &[]),
        plan_walkaround(p(0, 0), p(30, 0), Nm(1), r(-30, -30, 30, 30), &[r(31, 0, 32, 1)]),
        plan_walkaround(p(i64::MIN, 0), p(i64::MAX, 0), Nm(i64::MAX), r(-30, -30, 30, 30), &[]),
    ] {
        assert!(result.is_err());
    }
    assert!(plan_walkaround(p(0, 0), p(30, 0), Nm(1), r(-30, -30, 30, 30), &[r(15, 15, 16, 16); 65]).unwrap_err().contains("at most64"));
}
#[test]
fn dense_work_is_bounded_before_search_can_run_unchecked() {
    let obstacles: Vec<_> = (0..64).map(|i| r(10 + i * 10, 10 + i * 10, 11 + i * 10, 11 + i * 10)).collect();
    let result = plan_walkaround(p(1, 1), p(900, 900), Nm(1), r(0, 0, 1000, 1000), &obstacles);
    match result {
        Err(error) => assert!(error.contains("intersection-work limit"), "{error}"),
        Ok(plan) => {
            assert!(plan.work.intersection_tests <= 4_000_000);
            assert!(plan.work.graph_nodes <= 20_000);
            assert!(plan.work.candidate_edges <= 100_000);
            assert!(plan.waypoints.len() <= 510);
        }
    }
}
