//! Actual copper geometry, containment, coverage, limits and native determinism.
use viamind_doc::{BoardOutline, ObjectId, Pad, Pads, Part, Project, Side};
use viamind_drc::{Boundary, DrcInput, ViolationKind, run_drc};
use viamind_geom::{Nm, Point, Polygon};
fn part(designator: &str, x: f64, y: f64, width: f64, height: f64, angle: i32, side: Side, net: Option<&str>) -> Part {
    Part {
        schematic_symbol: None,
        id: ObjectId::new(),
        designator: designator.into(),
        value: "test".into(),
        footprint: "explicit".into(),
        position: Point::from_mm(x, y),
        rotation_mdeg: angle,
        side,
        pads: Some(
            Pads::try_from(vec![Pad {
                id: ObjectId::new(),
                number: "1".into(),
                position: Point::ORIGIN,
                width: Nm::from_mm(width),
                height: Nm::from_mm(height),
                net: net.map(str::to_owned),
            }])
            .unwrap(),
        ),
    }
}
fn project(parts: Vec<Part>) -> Project {
    let mut project = Project::new("DRC");
    project.board.parts = parts.into_iter().map(|p| (p.id, p)).collect();
    project
}
fn polygon(points: &[(f64, f64)]) -> Polygon {
    Polygon::new(points.iter().map(|(x, y)| Point::from_mm(*x, *y)).collect())
}
fn square(lo: f64, hi: f64) -> Polygon {
    polygon(&[(lo, lo), (hi, lo), (hi, hi), (lo, hi)])
}
fn rules(copper: f64, edge: f64) -> DrcInput {
    DrcInput { copper_clearance_mm: copper, edge_clearance_mm: edge, max_violations: 1000, ..Default::default() }
}
#[test]
fn copper_checks_real_rotated_rectangles_not_aabb_and_respects_layer_net_semantics() {
    let a = part("A", 0.0, 0.0, 10.0, 0.2, 45_000, Side::Top, Some("GND"));
    let b = part("B", 0.353553, 0.353553, 10.0, 0.2, 45_000, Side::Top, Some("VCC"));
    let mut p = project(vec![a, b]);
    let r = run_drc(&p, &rules(0.2, 0.0)).unwrap();
    assert_eq!(r.coverage.examined_candidate_pairs, 1);
    assert!(r.violations.is_empty());
    let b_id = p.board.part_by_designator("B").unwrap().id;
    p.board.parts.get_mut(&b_id).unwrap().position = Point::from_mm(0.176777, 0.176777);
    let r = run_drc(&p, &rules(0.2, 0.0)).unwrap();
    assert_eq!(r.violations.len(), 1);
    assert_eq!(r.violations[0].kind, ViolationKind::CopperClearance);
    assert!((r.violations[0].measured_clearance.to_mm() - 0.05).abs() < 0.00001);
    p.board.parts.get_mut(&b_id).unwrap().side = Side::Bottom;
    assert!(run_drc(&p, &rules(0.2, 0.0)).unwrap().violations.is_empty());
    let b = p.board.parts.get_mut(&b_id).unwrap();
    b.side = Side::Top;
    b.pads.as_mut().unwrap().0[0].net = Some("GND".into());
    assert!(run_drc(&p, &rules(0.2, 0.0)).unwrap().violations.is_empty());
    p.board.parts.get_mut(&b_id).unwrap().pads.as_mut().unwrap().0[0].net = None;
    assert_eq!(run_drc(&p, &rules(0.2, 0.0)).unwrap().violations.len(), 1);
}
#[test]
fn copper_touching_zero_rules_and_half_nm_edges_are_evaluated_without_rounding_geometry() {
    let mut a = part("A", 0.0, 0.0, 0.000003, 0.000003, 0, Side::Top, None);
    let mut b = a.clone();
    b.id = ObjectId::new();
    b.pads.as_mut().unwrap().0[0].id = ObjectId::new();
    b.designator = "B".into();
    b.position.x = Nm(4);
    let mut p = project(vec![a.clone(), b.clone()]);
    assert!(run_drc(&p, &rules(0.000001, 0.0)).unwrap().violations.is_empty()); // exact 1 Nm gap
    b.position.x = Nm(3);
    p = project(vec![a.clone(), b.clone()]);
    assert_eq!(run_drc(&p, &rules(0.0, 0.0)).unwrap().violations.len(), 1); // touching still shorts
    a.rotation_mdeg = 90_000;
    a.side = Side::Bottom;
    a.pads.as_mut().unwrap().0[0].position = Point::new(Nm(2), Nm(1));
    b.position = Point::new(Nm(1), Nm(2));
    b.side = Side::Bottom;
    assert_eq!(run_drc(&project(vec![a, b]), &rules(0.0, 0.0)).unwrap().violations.len(), 1); // mirrored X then CCW cardinal rotation
}
#[test]
fn substrate_checks_detect_enclosed_cutouts_concave_notches_tangency_and_rotated_tips() {
    let mut p = project(vec![part("A", 0.0, 0.0, 2.0, 2.0, 0, Side::Top, Some("GND"))]);
    p.board.outline = Some(BoardOutline::new(square(-10.0, 10.0), vec![square(-0.1, 0.1)]).unwrap());
    let r = run_drc(&p, &rules(0.0, 0.0)).unwrap();
    assert_eq!(r.violations[0].kind, ViolationKind::OutsideSubstrate);
    assert_eq!(r.violations[0].boundary, Some(Boundary::Cutout { index: 0 }));
    p.board.outline = Some(BoardOutline::new(square(-10.0, 10.0), vec![square(-3.0, 3.0)]).unwrap());
    assert_eq!(run_drc(&p, &rules(0.0, 0.0)).unwrap().violations[0].kind, ViolationKind::OutsideSubstrate);
    let id = *p.board.parts.keys().next().unwrap();
    let a = p.board.parts.get_mut(&id).unwrap();
    a.position = Point::from_mm(5.0, 5.0);
    a.pads.as_mut().unwrap().0[0].width = Nm::from_mm(4.0);
    a.pads.as_mut().unwrap().0[0].height = Nm::from_mm(4.0);
    p.board.outline = Some(
        BoardOutline::new(polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (6.0, 10.0), (6.0, 4.0), (4.0, 4.0), (4.0, 10.0), (0.0, 10.0)]), vec![])
            .unwrap(),
    );
    assert_eq!(run_drc(&p, &rules(0.0, 0.0)).unwrap().violations[0].kind, ViolationKind::OutsideSubstrate); // every corner is inside, notch passes between them
    p.board.outline = Some(BoardOutline::new(square(0.0, 10.0), vec![]).unwrap());
    let a = p.board.parts.get_mut(&id).unwrap();
    a.position = Point::from_mm(1.0, 5.0);
    a.pads.as_mut().unwrap().0[0].width = Nm::from_mm(2.0);
    a.pads.as_mut().unwrap().0[0].height = Nm::from_mm(2.0);
    assert!(run_drc(&p, &rules(0.0, 0.0)).unwrap().violations.is_empty());
    assert_eq!(run_drc(&p, &rules(0.0, 0.1)).unwrap().violations[0].kind, ViolationKind::BoardEdgeClearance);
    p.board.parts.get_mut(&id).unwrap().rotation_mdeg = 45_000;
    assert_eq!(run_drc(&p, &rules(0.0, 0.0)).unwrap().violations[0].kind, ViolationKind::OutsideSubstrate);
}
#[test]
fn edge_clearance_checks_cutout_boundaries_and_exact_threshold() {
    let mut p = project(vec![part("A", 2.0, 5.0, 2.0, 2.0, 0, Side::Top, None)]);
    p.board.outline = Some(BoardOutline::new(square(0.0, 10.0), vec![polygon(&[(3.2, 4.0), (4.0, 4.0), (4.0, 6.0), (3.2, 6.0)])]).unwrap());
    assert!(run_drc(&p, &rules(0.0, 0.2)).unwrap().violations.is_empty());
    let r = run_drc(&p, &rules(0.0, 0.200001)).unwrap();
    assert_eq!(r.violations.len(), 1);
    assert_eq!(r.violations[0].boundary, Some(Boundary::Cutout { index: 0 }));
    assert_eq!(r.violations[0].measured_clearance, Nm(200_000));
}
#[test]
fn coverage_limits_native_roundtrip_and_ordering_are_honest_and_deterministic() {
    let a = part("A", 0.0, 0.0, 2.0, 2.0, 0, Side::Top, None);
    let mut legacy = a.clone();
    legacy.id = ObjectId::new();
    legacy.designator = "Legacy".into();
    legacy.pads = None;
    let mut extreme = a.clone();
    extreme.id = ObjectId::new();
    extreme.designator = "Extreme".into();
    extreme.position.x = Nm(i64::MAX);
    extreme.pads.as_mut().unwrap().0[0].id = ObjectId::new();
    let mut b = a.clone();
    b.id = ObjectId::new();
    b.designator = "B".into();
    b.pads.as_mut().unwrap().0[0].id = ObjectId::new();
    let mut c = b.clone();
    c.id = ObjectId::new();
    c.designator = "C".into();
    c.pads.as_mut().unwrap().0[0].id = ObjectId::new();
    let extreme_id = extreme.id;
    let p = project(vec![a, b, c, legacy, extreme]);
    let before = p.clone();
    let r = run_drc(&p, &DrcInput::default()).unwrap();
    assert_eq!(p, before);
    assert_eq!(r.coverage.checked_pads, 3);
    assert_eq!(r.coverage.excluded_parts.len(), 2);
    assert!(r.coverage.excluded_parts.iter().any(|p| p.part_id == extreme_id && p.reason.contains("unsupported transform")));
    assert!(!r.coverage.explicit_outline_checked);
    assert_eq!(r.violations.len(), 3);
    assert!(!r.truncated);
    assert!(r.coverage.checks_not_performed.iter().any(|v| v.contains("curved tracks")));
    let restored = viamind_doc::file::from_str(&viamind_doc::file::to_string(&p)).unwrap();
    assert_eq!(run_drc(&restored, &DrcInput::default()).unwrap(), r);
    let limited = run_drc(&p, &DrcInput { max_violations: 1, ..Default::default() }).unwrap();
    assert_eq!(limited.violations.len(), 1);
    assert!(limited.truncated);
    assert!(limited.incomplete_reason.is_some());
    for input in
        [rules(-1.0, 0.0), rules(f64::NAN, 0.0), rules(0.0, f64::INFINITY), rules(100.1, 0.0), DrcInput { max_violations: 0, ..Default::default() }]
    {
        assert!(run_drc(&p, &input).is_err());
    }
}
#[test]
fn sparse_pad_broadphase_avoids_quadratic_pair_distance_evaluations() {
    let parts =
        (0..10_000).map(|i| part(&format!("R{i}"), f64::from(i % 100) * 5.0, f64::from(i / 100) * 5.0, 1.0, 1.0, 0, Side::Top, None)).collect();
    let report = run_drc(&project(parts), &DrcInput::default()).unwrap();
    assert_eq!(report.coverage.checked_pads, 10_000);
    assert_eq!(report.coverage.examined_candidate_pairs, 0);
    assert!(report.violations.is_empty());
}

#[test]
fn dense_exempt_copper_pairs_still_obey_the_work_budget_and_report_incomplete() {
    let parts = (0..1500).map(|i| part(&format!("R{i}"), 0.0, 0.0, 1.0, 1.0, 0, Side::Top, Some("GND"))).collect();
    let report = run_drc(&project(parts), &DrcInput::default()).unwrap();
    assert!(report.violations.is_empty());
    assert!(report.truncated);
    assert_eq!(report.coverage.examined_candidate_pairs, viamind_drc::MAX_CANDIDATE_PAIRS);
    assert!(report.incomplete_reason.unwrap().contains("candidate-pair limit"));
}

#[test]
fn legacy_heavy_coverage_keeps_total_and_bounds_actionable_details() {
    let mut legacy = part("Legacy", 0.0, 0.0, 1.0, 1.0, 0, Side::Top, None);
    legacy.pads = None;
    let parts = (0..1001)
        .map(|i| {
            let mut p = legacy.clone();
            p.id = ObjectId::new();
            p.designator = format!("L{i}");
            p
        })
        .collect();
    let report = run_drc(&project(parts), &DrcInput::default()).unwrap();
    assert_eq!(report.coverage.checked_pads, 0);
    assert_eq!(report.coverage.excluded_part_count, 1001);
    assert_eq!(report.coverage.excluded_parts.len(), viamind_drc::MAX_EXCLUDED_DETAILS);
    assert!(report.coverage.excluded_details_truncated);
    assert!(!report.coverage.explicit_outline_checked);
    assert!(!report.coverage.checks_not_performed.is_empty());
}
