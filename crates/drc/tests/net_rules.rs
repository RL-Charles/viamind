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
    NamedNetMinima { minimum_copper_clearance: Nm(clearance), minimum_track_width: Nm(width) }
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
