//! Moderate-board timing is evidence, not a universal desktop performance gate.
use std::time::{Duration, Instant};
use viamind_doc::{CopperLayer, ObjectId, Project, Track};
use viamind_engine::{AutoDrcConfig, AutoDrcPhase, EngineHandle, Origin};
use viamind_geom::{Nm, Point};

fn ready(engine: &EngineHandle) -> viamind_engine::AutoDrcStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = engine.auto_drc_status();
        if status.phase == AutoDrcPhase::Ready {
            return status;
        }
        assert!(Instant::now() < deadline, "automatic checker did not finish: {status:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn moderate_board_worker_timing_and_undo_redo_refresh_are_measured() {
    let mut project = Project::new("1000 straight tracks");
    for i in 0..1000 {
        let id = ObjectId::new();
        let y = Nm(i * 2_000_000);
        project.board.tracks.insert(
            id,
            Track {
                id,
                start: Point::new(Nm(0), y),
                end: Point::new(Nm(1_000_000), y),
                width: Nm(200_000),
                layer: CopperLayer::Top,
                net: Some("A".into()),
            },
        );
    }
    let engine = EngineHandle::new(project);
    engine.configure_auto_drc(AutoDrcConfig { enabled: true, ..Default::default() }).unwrap();
    let initial = ready(&engine);
    assert_eq!(initial.report.unwrap().coverage.checked_tracks, 1000);
    let start = Instant::now();
    engine.execute("add_via", serde_json::json!({"x_mm":50,"y_mm":50,"diameter_mm":0.6,"drill_mm":0.3,"net":"B"}), Origin::User).unwrap();
    let edit_ms = start.elapsed().as_secs_f64() * 1000.0;
    let updated = ready(&engine);
    assert_eq!(updated.generation, 2);
    assert_eq!(updated.report.as_ref().unwrap().coverage.checked_vias, 1);
    eprintln!(
        "1000-track automatic DRC: edit-return={edit_ms:.3} ms, snapshot={:.3} ms, checker={:.3} ms, request-to-publication={:.3} ms; excludes native presentation",
        updated.snapshot_capture_ms.unwrap(),
        updated.check_ms.unwrap(),
        updated.request_to_publication_ms.unwrap()
    );
    engine.undo(Origin::User).unwrap();
    let undone = ready(&engine);
    assert_eq!(undone.generation, 3);
    assert_eq!(undone.report.unwrap().coverage.checked_vias, 0);
    engine.redo(Origin::User).unwrap();
    let redone = ready(&engine);
    assert_eq!(redone.generation, 4);
    assert_eq!(redone.report.unwrap().coverage.checked_vias, 1);
}

#[test]
fn named_policy_edit_and_undo_refresh_captured_automatic_checker_sources() {
    let mut project = Project::new("Automatic exact minima");
    let id = ObjectId::new();
    project.board.tracks.insert(
        id,
        Track { id, start: Point::ORIGIN, end: Point::new(Nm(1_000_000), Nm(0)), width: Nm(300_000), layer: CopperLayer::Top, net: Some("A".into()) },
    );
    let e = EngineHandle::new(project);
    e.configure_auto_drc(AutoDrcConfig { enabled: true, ..Default::default() }).unwrap();
    assert!(ready(&e).report.unwrap().document_net_rules.is_empty());
    e.execute_typed(
        viamind_engine::SetNamedNetRules {
            expected_project: e.read(|p| p.id),
            expected_cursor: e.event_cursor(),
            rules: vec![viamind_engine::NamedNetRuleRow {
                net: "A".into(),
                minimum_copper_clearance: Nm(0),
                minimum_track_width: Nm(300_001),
                minimum_via_drill: viamind_geom::Nm(0),
                minimum_annular_width: viamind_geom::Nm(0),
            }],
        },
        Origin::User,
    )
    .unwrap();
    let updated = ready(&e);
    assert_eq!(updated.source_cursor, Some(e.event_cursor()));
    let report = updated.report.unwrap();
    assert_eq!(report.document_net_rules, e.read(|p| p.board.net_rules.clone()));
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.violations[0].kind, viamind_drc::ViolationKind::MinimumTrackWidth);
    e.undo(Origin::User).unwrap();
    let report = ready(&e).report.unwrap();
    assert!(report.document_net_rules.is_empty() && report.violations.is_empty());
}
#[test]
fn via_policy_edit_and_undo_refresh_exact_automatic_sources_and_geometry() {
    let mut p = Project::new("Automatic via minima");
    let via = viamind_doc::Via { id: ObjectId::new(), position: Point::ORIGIN, diameter: Nm(600001), drill: Nm(300000), net: Some("A".into()) };
    p.board.vias.insert(via.id, via.clone());
    let e = EngineHandle::new(p);
    e.configure_auto_drc(AutoDrcConfig { enabled: true, ..Default::default() }).unwrap();
    assert!(ready(&e).report.unwrap().violations.is_empty());
    let before = e.session_state();
    e.execute_typed(
        viamind_engine::SetNamedNetRules {
            expected_project: before.project.id,
            expected_cursor: before.cursor,
            rules: vec![viamind_engine::NamedNetRuleRow {
                net: "A".into(),
                minimum_copper_clearance: Nm(0),
                minimum_track_width: Nm(1),
                minimum_via_drill: Nm(300001),
                minimum_annular_width: Nm(150001),
            }],
        },
        Origin::User,
    )
    .unwrap();
    let status = ready(&e);
    assert_eq!(status.source_cursor, Some(e.event_cursor()));
    let report = status.report.unwrap();
    assert_eq!(report.document_net_rules, e.read(|p| p.board.net_rules.clone()));
    assert_eq!(
        report.violations.iter().map(|v| v.kind).collect::<Vec<_>>(),
        vec![viamind_drc::ViolationKind::MinimumViaDrill, viamind_drc::ViolationKind::MinimumAnnularWidth]
    );
    assert_eq!(e.read(|p| p.board.vias[&via.id].clone()), via);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.session_state().project, before.project);
    let report = ready(&e).report.unwrap();
    assert!(report.document_net_rules.is_empty() && report.violations.is_empty());
}
