//! Independently chosen track/circular-hole fixtures exercise actual planar copper.
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_drc::{DrcInput, ViolationKind, run_drc};
use viamind_geom::{Nm, Point, Polygon};
fn track(a: (f64, f64), b: (f64, f64), width: f64, layer: CopperLayer, net: Option<&str>) -> Track {
    Track {
        id: ObjectId::new(),
        start: Point::from_mm(a.0, a.1),
        end: Point::from_mm(b.0, b.1),
        width: Nm::from_mm(width),
        layer,
        net: net.map(str::to_owned),
    }
}
fn via(x: f64, y: f64, diameter: f64, drill: f64, net: Option<&str>) -> Via {
    Via {
        id: ObjectId::new(),
        position: Point::from_mm(x, y),
        diameter: Nm::from_mm(diameter),
        drill: Nm::from_mm(drill),
        net: net.map(str::to_owned),
    }
}
fn part() -> Part {
    Part {
        schematic_symbol: None,
        id: ObjectId::new(),
        designator: "P1".into(),
        value: "test".into(),
        footprint: "explicit".into(),
        position: Point::ORIGIN,
        rotation_mdeg: 0,
        side: Side::Top,
        pads: Some(
            Pads::try_from(vec![Pad {
                id: ObjectId::new(),
                number: "1".into(),
                position: Point::ORIGIN,
                width: Nm::from_mm(1.0),
                height: Nm::from_mm(1.0),
                net: Some("B".into()),
            }])
            .unwrap(),
        ),
    }
}
fn project(tracks: Vec<Track>, vias: Vec<Via>) -> Project {
    let mut p = Project::new("Copper DRC");
    p.board.tracks = tracks.into_iter().map(|t| (t.id, t)).collect();
    p.board.vias = vias.into_iter().map(|v| (v.id, v)).collect();
    p
}
fn rules(clearance: f64) -> DrcInput {
    DrcInput { copper_clearance_mm: clearance, edge_clearance_mm: 0.0, ..Default::default() }
}
fn count(p: &Project, rules: &DrcInput, kind: ViolationKind) -> usize {
    run_drc(p, rules).unwrap().violations.iter().filter(|v| v.kind == kind).count()
}
fn square(lo: f64, hi: f64) -> Polygon {
    Polygon::new([(lo, lo), (hi, lo), (hi, hi), (lo, hi)].into_iter().map(|(x, y)| Point::from_mm(x, y)).collect())
}
#[test]
fn capsule_round_caps_exact_gap_layers_nets_and_unassigned_policy() {
    let a = track((0.0, 0.0), (1.0, 0.0), 0.2, CopperLayer::Top, Some("A"));
    let b = track((1.4, 0.0), (2.0, 0.0), 0.2, CopperLayer::Top, Some("B"));
    let id = b.id;
    let mut p = project(vec![a, b], vec![]);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0);
    let r = run_drc(&p, &rules(0.200001)).unwrap();
    assert_eq!(r.violations[0].measured_clearance, Nm(200_000));
    p.board.tracks.get_mut(&id).unwrap().start.x = Nm::from_mm(1.2);
    assert_eq!(count(&p, &rules(0.0), ViolationKind::CopperClearance), 1);
    p.board.tracks.get_mut(&id).unwrap().layer = CopperLayer::Bottom;
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0);
    let b = p.board.tracks.get_mut(&id).unwrap();
    b.layer = CopperLayer::Top;
    b.net = Some("A".into());
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0);
    p.board.tracks.get_mut(&id).unwrap().net = None;
    assert_eq!(count(&p, &rules(0.0), ViolationKind::CopperClearance), 1);
}
#[test]
fn diagonal_capsules_and_rotated_pad_use_geometry_instead_of_bounds() {
    let a = track((-5.0, 5.0), (5.0, -5.0), 0.2, CopperLayer::Top, None);
    let b = track((-4.646447, 5.353553), (5.353553, -4.646447), 0.2, CopperLayer::Top, None);
    let mut p = project(vec![a, b], vec![]);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0);
    let mut pad = part();
    pad.rotation_mdeg = 45_000;
    let geometry = &mut pad.pads.as_mut().unwrap().0[0];
    geometry.width = Nm::from_mm(4.0);
    geometry.height = Nm::from_mm(0.2);
    p.board.tracks.retain(|_, t| t.start.x.0 < -4_800_000);
    p.board.tracks.values_mut().next().unwrap().start.y += Nm::from_mm(0.5);
    p.board.tracks.values_mut().next().unwrap().end.y += Nm::from_mm(0.5);
    p.board.parts.insert(pad.id, pad);
    assert_eq!(count(&p, &rules(0.1), ViolationKind::CopperClearance), 0);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 1);
}
#[test]
fn annular_copper_excludes_drill_interiors_but_physical_hole_checks_remain() {
    let outer = via(0.0, 0.0, 10.0, 8.0, Some("A"));
    let inner = via(0.0, 0.0, 2.0, 1.0, Some("B"));
    let mut p = project(vec![track((-0.5, 0.0), (0.5, 0.0), 0.1, CopperLayer::Top, Some("B"))], vec![outer, inner]);
    let pad = part();
    p.board.parts.insert(pad.id, pad);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0); // inner same-net pad/track; both well inside outer drill
    assert!(count(&p, &rules(0.2), ViolationKind::HoleToCopperClearance) > 0);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::HoleClearance), 1);
    p.board.parts.clear();
    p.board.tracks.clear();
    let r = run_drc(&p, &rules(3.000001)).unwrap();
    assert!(r.violations.iter().any(|v| v.kind == ViolationKind::CopperClearance && v.measured_clearance == Nm::from_mm(3.0)));
    assert_eq!(count(&p, &rules(3.0), ViolationKind::CopperClearance), 0);
}
#[test]
fn through_vias_share_both_layers_and_same_net_holes_still_need_spacing() {
    let a = via(0.0, 0.0, 0.6, 0.3, Some("A"));
    let b = via(0.5, 0.0, 0.6, 0.3, Some("A"));
    let t = track((-1.0, 0.0), (1.0, 0.0), 0.1, CopperLayer::Bottom, Some("A"));
    let id = t.id;
    let mut p = project(vec![t], vec![a, b]);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::HoleToCopperClearance), 0);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::HoleClearance), 1); // 0.2mm gap below 0.25, independent of net
    p.board.tracks.get_mut(&id).unwrap().net = None;
    assert_eq!(count(&p, &rules(0.0), ViolationKind::CopperClearance), 2);
    assert_eq!(count(&p, &rules(0.0), ViolationKind::HoleToCopperClearance), 2);
    let r = run_drc(&p, &rules(0.2)).unwrap();
    assert_eq!((r.coverage.checked_tracks, r.coverage.checked_vias, r.coverage.checked_holes), (1, 2, 2));
}
#[test]
fn minimum_drill_annular_width_and_half_nm_thresholds_are_exact() {
    let mut v = via(0.0, 0.0, 0.6, 0.3, None);
    let id = v.id;
    let mut p = project(vec![], vec![v.clone()]);
    let mut r = rules(0.0);
    r.minimum_via_drill_mm = 0.3;
    r.minimum_annular_width_mm = 0.15;
    assert!(run_drc(&p, &r).unwrap().violations.is_empty());
    r.minimum_via_drill_mm = 0.300001;
    r.minimum_annular_width_mm = 0.150001;
    let result = run_drc(&p, &r).unwrap();
    assert_eq!(result.violations.len(), 2);
    assert!(result.violations.iter().any(|v| v.kind == ViolationKind::MinimumViaDrill && v.measured_clearance == Nm(300_000)));
    v.diameter = Nm(601);
    v.drill = Nm(300);
    p.board.vias.insert(id, v);
    r.minimum_via_drill_mm = 0.0;
    r.minimum_annular_width_mm = 0.000150;
    assert!(run_drc(&p, &r).unwrap().violations.is_empty());
    r.minimum_annular_width_mm = 0.000151;
    assert_eq!(run_drc(&p, &r).unwrap().violations[0].measured_clearance, Nm(150));
}
#[test]
fn round_caps_and_actual_annuli_check_substrate_not_centres_or_outer_disks() {
    let t = track((1.0, 5.0), (9.5, 5.0), 1.0, CopperLayer::Top, None);
    let id = t.id;
    let mut p = project(vec![t], vec![]);
    p.board.outline = Some(BoardOutline::new(square(0.0, 10.0), vec![]).unwrap());
    assert!(run_drc(&p, &rules(0.0)).unwrap().violations.is_empty());
    p.board.tracks.get_mut(&id).unwrap().end.x += Nm(1);
    assert_eq!(count(&p, &rules(0.0), ViolationKind::OutsideSubstrate), 1);
    p.board.tracks.clear();
    let v = via(5.0, 5.0, 4.0, 2.0, None);
    p.board.vias.insert(v.id, v);
    p.board.outline = Some(BoardOutline::new(square(0.0, 10.0), vec![square(4.8, 5.2)]).unwrap());
    assert!(run_drc(&p, &rules(0.0)).unwrap().violations.is_empty()); // cutout entirely inside drill
    p.board.outline = Some(
        BoardOutline::new(
            square(0.0, 10.0),
            vec![Polygon::new([(6.2, 4.9), (6.4, 4.9), (6.4, 5.1), (6.2, 5.1)].into_iter().map(|(x, y)| Point::from_mm(x, y)).collect())],
        )
        .unwrap(),
    );
    assert_eq!(count(&p, &rules(0.0), ViolationKind::OutsideSubstrate), 1); // small cutout in annular copper
}
#[test]
fn mixed_native_roundtrip_limits_and_invalid_new_rules_are_read_only() {
    let p = project(vec![track((0.0, 0.0), (2.0, 0.0), 0.2, CopperLayer::Top, None)], vec![via(0.0, 0.0, 0.6, 0.3, None)]);
    let before = p.clone();
    let r = run_drc(&p, &DrcInput::default()).unwrap();
    let restored = viamind_doc::file::from_str(&viamind_doc::file::to_string(&p)).unwrap();
    assert_eq!(run_drc(&restored, &DrcInput::default()).unwrap(), r);
    assert_eq!(p, before);
    assert!(run_drc(&p, &DrcInput { max_violations: 1, ..Default::default() }).unwrap().truncated);
    for invalid in [
        DrcInput { minimum_via_drill_mm: f64::NAN, ..Default::default() },
        DrcInput { minimum_annular_width_mm: -1.0, ..Default::default() },
        DrcInput { hole_clearance_mm: f64::INFINITY, ..Default::default() },
        DrcInput { hole_to_copper_clearance_mm: 101.0, ..Default::default() },
    ] {
        assert!(run_drc(&p, &invalid).is_err());
        assert_eq!(p, before);
    }
    let mut bad = p.clone();
    bad.board.vias.values_mut().next().unwrap().drill = Nm(0);
    assert!(run_drc(&bad, &DrcInput::default()).unwrap_err().to_string().contains("via"));
}

#[test]
fn offset_annuli_and_inside_drill_board_edge_clearance_have_exact_geometric_gaps() {
    let a = via(0.0, 0.0, 0.6, 0.3, Some("A"));
    let b = via(0.8, 0.0, 0.6, 0.3, Some("B"));
    let p = project(vec![], vec![a, b]);
    assert_eq!(count(&p, &rules(0.2), ViolationKind::CopperClearance), 0);
    let r = run_drc(&p, &rules(0.200001)).unwrap();
    assert!(r.violations.iter().any(|v| v.kind == ViolationKind::CopperClearance && v.measured_clearance == Nm(200_000)));
    let a = via(0.0, 0.0, 6.0, 4.0, Some("A"));
    let b = via(0.5, 0.0, 2.0, 1.0, Some("B"));
    let p = project(vec![], vec![a, b]);
    assert_eq!(count(&p, &rules(0.5), ViolationKind::CopperClearance), 0);
    let r = run_drc(&p, &rules(0.500001)).unwrap();
    assert!(r.violations.iter().any(|v| v.kind == ViolationKind::CopperClearance && v.measured_clearance == Nm(500_000)));
    let mut p = project(vec![], vec![via(5.0, 5.0, 4.0, 2.0, None)]);
    p.board.outline = Some(BoardOutline::new(square(0.0, 10.0), vec![square(4.8, 5.2)]).unwrap());
    let mut r = rules(0.0);
    r.edge_clearance_mm = 0.7;
    assert!(run_drc(&p, &r).unwrap().violations.is_empty());
    r.edge_clearance_mm = 0.72;
    let report = run_drc(&p, &r).unwrap();
    assert_eq!(report.violations[0].kind, ViolationKind::BoardEdgeClearance);
    assert!((report.violations[0].measured_clearance.to_mm() - (1.0 - 0.2_f64.hypot(0.2))).abs() < 0.000001);
}
#[test]
fn sparse_mixed_ten_thousand_objects_skip_quadratic_pair_checks() {
    let mut p = Project::new("sparse mixed");
    for i in 0..5000 {
        let x = f64::from(i % 100) * 10.0;
        let y = f64::from(i / 100) * 10.0;
        let t = track((x, y), (x + 1.0, y), 0.2, CopperLayer::Top, None);
        p.board.tracks.insert(t.id, t);
        let v = via(x + 5.0, y, 0.6, 0.3, None);
        p.board.vias.insert(v.id, v);
    }
    let report = run_drc(&p, &DrcInput::default()).unwrap();
    assert!(report.violations.is_empty());
    assert_eq!(report.coverage.examined_candidate_pairs, 0);
    assert_eq!((report.coverage.checked_tracks, report.coverage.checked_vias), (5000, 5000));
}
