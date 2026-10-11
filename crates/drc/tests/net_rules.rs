use viamind_doc::{CopperLayer, NamedNetMinima, ObjectId, Project, Track};
use viamind_drc::{DrcInput, ViolationKind, run_drc};
use viamind_geom::{Nm, Point};
fn track(y: i64, net: Option<&str>, width: i64) -> Track {
    Track {
        id: ObjectId::new(),
        start: Point::new(Nm(0), Nm(y)),
        end: Point::new(Nm(1_000_000), Nm(y)),
        width: Nm(width),
        layer: CopperLayer::Top,
        net: net.map(str::to_owned),
    }
}
fn rule(clearance: i64, width: i64) -> NamedNetMinima {
    NamedNetMinima {
        minimum_copper_clearance: Nm(clearance),
        minimum_track_width: Nm(width),
        minimum_via_drill: viamind_geom::Nm(0),
        minimum_annular_width: viamind_geom::Nm(0),
    }
}
#[test]
fn far_pairs_are_discovered_but_unrelated_large_rules_do_not_pollute_pair_minima() {
    let mut project = Project::new("Rules");
    for t in [track(0, Some("A"), 200_000), track(1_000_000, Some("B"), 200_000)] {
        project.board.tracks.insert(t.id, t);
    }
    let input = DrcInput::default();
    assert!(run_drc(&project, &input).unwrap().violations.is_empty());
    project.board.net_rules.insert("UNUSED".into(), rule(10_000_000, 1));
    assert!(run_drc(&project, &input).unwrap().violations.is_empty());
    project.board.net_rules.insert("A".into(), rule(900_000, 200_000));
    project.board.net_rules.insert("B".into(), rule(700_000, 200_000));
    let report = run_drc(&project, &input).unwrap();
    assert_eq!(report.violations.len(), 1);
    let issue = &report.violations[0];
    assert_eq!(issue.kind, ViolationKind::CopperClearance);
    assert_eq!(issue.measured_clearance, Nm(800_000));
    assert_eq!(issue.required_clearance, Nm(900_000));
    assert_eq!(report.document_net_rules, project.board.net_rules);
    project.board.net_rules.get_mut("A").unwrap().minimum_copper_clearance = Nm(800_000);
    assert!(run_drc(&project, &input).unwrap().violations.is_empty());
    let stronger = DrcInput { copper_clearance_mm: 0.800001, ..input };
    assert_eq!(run_drc(&project, &stronger).unwrap().violations[0].required_clearance, Nm(800_001));
}
#[test]
fn exact_width_floor_equality_unassigned_and_case_sensitive_names() {
    for (name, width, expected) in [(Some("A"), 299_999, 1), (Some("A"), 300_000, 0), (Some("a"), 1, 0), (None, 1, 0)] {
        let mut project = Project::new("Widths");
        let t = track(0, name, width);
        let id = t.id;
        project.board.tracks.insert(id, t);
        project.board.net_rules.insert("A".into(), rule(0, 300_000));
        let r = run_drc(&project, &DrcInput::default()).unwrap();
        assert_eq!(r.violations.len(), expected);
        if expected > 0 {
            assert_eq!(r.violations[0].kind, ViolationKind::MinimumTrackWidth);
            assert_eq!(r.violations[0].objects[0].selection_id(), id);
            assert_eq!(r.violations[0].measured_clearance, Nm(width));
        }
    }
}
#[test]
fn empty_policy_preserves_report_bytes_and_old_report_deserialization() {
    let project = Project::new("Empty");
    let report = run_drc(&project, &DrcInput::default()).unwrap();
    let bytes = serde_json::to_string(&report).unwrap();
    assert!(!bytes.contains("document_net_rules"));
    assert_eq!(serde_json::from_str::<viamind_drc::DrcReport>(&bytes).unwrap(), report);
}
#[test]
fn exact_via_drill_and_half_nm_annulus_use_matching_maxima_without_resizing() {
    // Outer minus drill is twice the true radial annulus. Both odd sides matter.
    for (net, drill, difference, request_drill, request_annulus, expected) in [
        (Some("A"), 300000, 300002, 0., 0., vec![ViolationKind::MinimumViaDrill]),
        (Some("A"), 300001, 300001, 0., 0., vec![ViolationKind::MinimumAnnularWidth]),
        (Some("A"), 300001, 300002, 0., 0., vec![]),
        (Some("A"), 300002, 300003, 0., 0., vec![]),
        (Some("a"), 1, 1, 0., 0., vec![]),
        (None, 1, 1, 0., 0., vec![]),
        (Some("A"), 300001, 300002, 0.300002, 0., vec![ViolationKind::MinimumViaDrill]),
        (Some("A"), 300001, 300002, 0., 0.150002, vec![ViolationKind::MinimumAnnularWidth]),
    ] {
        let mut p = Project::new("Via floors");
        let via = viamind_doc::Via {
            id: ObjectId::new(),
            position: Point::new(Nm(0), Nm(0)),
            diameter: Nm(drill + difference),
            drill: Nm(drill),
            net: net.map(str::to_owned),
        };
        let original = via.clone();
        p.board.vias.insert(via.id, via);
        p.board.net_rules.insert("A".into(), NamedNetMinima { minimum_via_drill: Nm(300001), minimum_annular_width: Nm(150001), ..rule(0, 1) });
        let input = DrcInput { minimum_via_drill_mm: request_drill, minimum_annular_width_mm: request_annulus, ..Default::default() };
        let report = run_drc(&p, &input).unwrap();
        assert_eq!(report.violations.iter().map(|v| v.kind.clone()).collect::<Vec<_>>(), expected);
        assert_eq!(p.board.vias[&original.id], original);
        assert_eq!(report.document_net_rules, p.board.net_rules);
        for v in report.violations {
            assert_eq!(v.objects[0].selection_id(), original.id);
            assert!(v.measured_clearance < v.required_clearance);
        }
    }
}
#[test]
fn zero_via_fields_keep_legacy_report_policy_bytes_and_half_nm_decisions() {
    let mut p = Project::new("Old report");
    p.board.net_rules.insert("A".into(), rule(0, 1));
    for difference in [299999, 300000, 300001] {
        let via = viamind_doc::Via {
            id: ObjectId::new(),
            position: Point::ORIGIN,
            diameter: Nm(300000 + difference),
            drill: Nm(300000),
            net: Some("A".into()),
        };
        p.board.vias.clear();
        p.board.vias.insert(via.id, via);
        let report = run_drc(&p, &DrcInput { minimum_annular_width_mm: 0.15, ..Default::default() }).unwrap();
        assert_eq!(report.violations.len(), usize::from(difference < 300000));
        let text = serde_json::to_string(&report).unwrap();
        let expected = r#""document_net_rules":{"A":{"minimum_copper_clearance":0,"minimum_track_width":1}}"#;
        assert!(text.contains(expected));
        let restored: viamind_drc::DrcReport = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string(&restored).unwrap(), text);
    }
}
