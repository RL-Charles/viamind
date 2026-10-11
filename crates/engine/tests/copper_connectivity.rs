//! Positive endpoint proofs and conservative exclusions, independently assembled geometry.
use viamind_doc::{CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_engine::{ConnectivityQuery, EngineHandle, NetQuery, Origin, PadEndpointId, RoutePadPolyline, connection_guides};
use viamind_geom::{Nm, Point};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn part(project: &mut Project, x: i64, y: i64, side: Side) -> PadEndpointId {
    let pad =
        Pad { id: ObjectId::new(), number: "1".into(), position: Point::ORIGIN, width: Nm(1_000_000), height: Nm(1_000_000), net: Some("N".into()) };
    let part = Part {
        assembly: None,
        id: ObjectId::new(),
        designator: format!("J{}", project.board.parts.len()),
        value: String::new(),
        footprint: String::new(),
        position: p(x, y),
        rotation_mdeg: 0,
        side,
        pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
        schematic_symbol: None,
    };
    let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
    project.board.parts.insert(part.id, part);
    id
}
fn track(project: &mut Project, a: Point, b: Point, layer: CopperLayer, net: &str) {
    let track = Track { id: ObjectId::new(), start: a, end: b, width: Nm(250_000), layer, net: Some(net.into()) };
    project.board.tracks.insert(track.id, track);
}
fn query(project: &Project) -> viamind_engine::ConnectivityResult {
    ConnectivityQuery { net: "N".into(), offset: 0, limit: 1000 }.run(project).unwrap()
}
#[test]
fn chains_merge_islands_and_leave_a_deterministic_residual_forest() {
    let mut project = Project::new("Partial island forest");
    for x in [10, 20, 30, 40, 50] {
        part(&mut project, x * 1_000_000, 10_000_000, Side::Top);
    }
    track(&mut project, p(10_000_000, 10_000_000), p(15_000_000, 20_000_000), CopperLayer::Top, "N");
    track(&mut project, p(15_000_000, 20_000_000), p(30_000_000, 10_000_000), CopperLayer::Top, "N");
    track(&mut project, p(40_000_000, 10_000_000), p(50_000_000, 10_000_000), CopperLayer::Top, "N");
    let before = project.clone();
    let result = query(&project);
    assert_eq!((result.terminal_count, result.guide_count), (5, 2));
    assert_eq!(result, query(&project));
    assert_eq!(connection_guides(&project), result.guides);
    assert_eq!(NetQuery { after: None, limit: 100 }.run(&project).unwrap().nets[0].guide_count, 2);
    assert_eq!(result.copper_proof.analyzed_tracks, 3);
    assert_eq!(result.copper_proof.supported_named_pads, 5);
    assert!(!result.copper_proof.budget_fallback);
    assert_eq!(project, before);
}
#[test]
fn one_nanometer_gap_wrong_side_net_and_interior_crossings_never_prove_a_join() {
    for mode in 0..6 {
        let mut project = Project::new("Conservative contact");
        part(&mut project, 10_000_000, 10_000_000, Side::Top);
        part(&mut project, 30_000_000, 10_000_000, Side::Top);
        track(&mut project, p(10_000_000, 10_000_000), p(20_000_000, 10_000_000), CopperLayer::Top, "N");
        let a = if mode == 0 {
            p(20_000_001, 10_000_000)
        } else if mode == 3 {
            p(15_000_000, 10_000_000)
        } else {
            p(20_000_000, 10_000_000)
        };
        track(
            &mut project,
            a,
            p(30_000_000, 10_000_000),
            if mode == 1 { CopperLayer::Bottom } else { CopperLayer::Top },
            if mode == 2 { "n" } else { "N" },
        );
        if mode == 4 {
            project.board.parts.values_mut().next().unwrap().rotation_mdeg = 45_000;
        }
        if mode == 5 {
            project.board.parts.values_mut().next().unwrap().pads.as_mut().unwrap().0[0].width = Nm(0);
        }
        let result = query(&project);
        assert_eq!(result.guide_count, 1, "mode{mode}");
        if mode >= 4 {
            assert_eq!(result.copper_proof.unsupported_named_pads, 1);
        }
    }
}
#[test]
fn mirrored_quarter_turn_exact_off_grid_routes_disappear_and_undo_restores_guides() {
    let mut project = Project::new("Exact proof with shared undo");
    project.board.outline = Some(
        serde_json::from_value(
            serde_json::json!({"outer":[{"x":0,"y":0},{"x":100000000,"y":0},{"x":100000000,"y":100000000},{"x":0,"y":100000000}],"cutouts":[]}),
        )
        .unwrap(),
    );
    let ids = [10_000_001, 30_000_007].map(|x| part(&mut project, x, 10_000_003, Side::Bottom));
    for part in project.board.parts.values_mut() {
        part.rotation_mdeg = 90_000;
        part.pads.as_mut().unwrap().0[0].position = p(123, 257);
    }
    let e = EngineHandle::new(project);
    let before = e.session_state();
    e.execute_typed(
        RoutePadPolyline {
            expected_project: before.project.id,
            expected_cursor: before.cursor,
            start: ids[0],
            target: ids[1],
            interior: vec![],
            grid: Nm(250_000),
            width: Nm(250_000),
            rules: Default::default(),
        },
        Origin::agent("router"),
    )
    .unwrap();
    let after = e.session_state();
    let events = e.subscribe();
    assert_eq!(query(&after.project).guide_count, 0);
    assert!(e.route_endpoint_snapshot().unwrap().guide_endpoints.is_empty());
    assert_eq!(e.query_nets(&NetQuery { after: None, limit: 100 }).unwrap().nets[0].guide_count, 0);
    assert_eq!(e.session_state(), after);
    assert!(events.try_recv().is_err());
    let bytes = viamind_doc::file::to_string(&after.project);
    let loaded = viamind_doc::file::from_str(&bytes).unwrap();
    assert_eq!(query(&loaded).guide_count, 0);
    e.undo(Origin::agent("router")).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    assert_eq!(query(&before.project).guide_count, 1);
    e.redo(Origin::agent("router")).unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
    assert_eq!(query(&e.read(Clone::clone)).guide_count, 0);
}
#[test]
fn through_via_bridges_exact_sides_only_with_annulus_reaching_incident_tracks() {
    for mode in 0..6 {
        let mut project = Project::new("Via endpoint proof");
        part(&mut project, 10_000_000, 10_000_000, Side::Top);
        part(&mut project, 30_000_000, 10_000_000, Side::Bottom);
        let center = p(20_000_000, 10_000_000);
        track(&mut project, p(10_000_000, 10_000_000), center, CopperLayer::Top, "N");
        track(&mut project, if mode == 1 { p(20_000_001, 10_000_000) } else { center }, p(30_000_000, 10_000_000), CopperLayer::Bottom, "N");
        let via = Via {
            id: ObjectId::new(),
            position: center,
            diameter: Nm(600_000),
            drill: Nm(300_000),
            net: Some(if mode == 2 { "other" } else { "N" }.into()),
        };
        project.board.vias.insert(via.id, via);
        if mode == 3 {
            project.board.tracks.retain(|_, t| t.layer == CopperLayer::Top);
            track(&mut project, center, p(20_000_010, 10_000_000), CopperLayer::Bottom, "N");
            track(&mut project, p(20_000_010, 10_000_000), p(30_000_000, 10_000_000), CopperLayer::Bottom, "N");
        }
        if mode == 4 {
            project.board.vias.values_mut().next().unwrap().drill = Nm(600_000);
        }
        if mode == 5 {
            let other = Via {
                id: ObjectId::new(),
                position: p(20_350_000, 10_000_000),
                diameter: Nm(600_000),
                drill: Nm(300_000),
                net: Some("foreign".into()),
            };
            project.board.vias.insert(other.id, other);
            assert_eq!(query(&project).copper_proof.analyzed_vias, 0);
        }
        assert_eq!(query(&project).guide_count, usize::from(mode != 0), "mode{mode}");
    }
}
#[test]
fn count_and_text_budgets_disable_all_suppression_without_mutation() {
    for mode in 0..3 {
        let mut project = Project::new("Proof fallback");
        let a = part(&mut project, 10_000_000, 10_000_000, Side::Top);
        part(&mut project, 30_000_000, 10_000_000, Side::Top);
        track(&mut project, p(10_000_000, 10_000_000), p(30_000_000, 10_000_000), CopperLayer::Top, "N");
        assert_eq!(query(&project).guide_count, 0);
        if mode == 0 {
            project.board.parts.get_mut(&a.part_id).unwrap().designator = "x".repeat(4097);
        } else {
            let n = if mode == 1 { 100_001 } else { 600 };
            for i in 0..n {
                let mut t = Track { id: ObjectId::new(), start: p(0, 0), end: p(1, 1), width: Nm(1), layer: CopperLayer::Top, net: None };
                if mode == 2 {
                    t.net = Some(format!("{i:04}{}", "x".repeat(4092)));
                }
                project.board.tracks.insert(t.id, t);
            }
        }
        let before = project.clone();
        let result = query(&project);
        assert!(result.copper_proof.budget_fallback, "mode{mode}");
        assert_eq!(result.guide_count, 1);
        assert_eq!(result.copper_proof.analyzed_tracks, 0);
        assert_eq!(project, before);
    }
}
#[test]
fn drilled_away_shapes_and_foreign_midsegment_holes_never_hide_guides() {
    for tiny in [true, false] {
        let mut project = Project::new("Drilled copper is unsupported");
        let (a, b, center) =
            if tiny { (p(0, 0), p(10, 0), p(5, 0)) } else { (p(10_000_000, 10_000_000), p(30_000_000, 10_000_000), p(20_000_000, 10_000_000)) };
        for point in [a, b] {
            let id = part(&mut project, point.x.0, point.y.0, Side::Top);
            if tiny {
                let pad = &mut project.board.parts.get_mut(&id.part_id).unwrap().pads.as_mut().unwrap().0[0];
                pad.width = Nm(1);
                pad.height = Nm(1);
            }
        }
        track(&mut project, a, b, CopperLayer::Top, "N");
        if tiny {
            project.board.tracks.values_mut().next().unwrap().width = Nm(1);
        }
        assert_eq!(query(&project).guide_count, 0);
        let via = Via { id: ObjectId::new(), position: center, diameter: Nm(600_000), drill: Nm(300_000), net: Some("foreign".into()) };
        project.board.vias.insert(via.id, via);
        let result = query(&project);
        assert_eq!(result.guide_count, 1);
        assert_eq!(result.copper_proof.analyzed_tracks, 0);
        if tiny {
            assert_eq!(result.copper_proof.unsupported_named_pads, 2);
        }
    }
}
#[test]
fn dense_hole_candidates_and_invalid_via_geometry_disable_all_proofs_explicitly() {
    for invalid in [false, true] {
        let mut project = Project::new("Hole proof fallback");
        part(&mut project, 10_000_000, 10_000_000, Side::Top);
        part(&mut project, 30_000_000, 10_000_000, Side::Top);
        track(&mut project, p(10_000_000, 10_000_000), p(30_000_000, 10_000_000), CopperLayer::Top, "N");
        assert_eq!(query(&project).guide_count, 0);
        for _ in 0..if invalid { 1 } else { 1001 } {
            let via = Via {
                id: ObjectId::new(),
                position: p(50_000_000, 50_000_000),
                diameter: Nm(600_000),
                drill: Nm(if invalid { 600_000 } else { 300_000 }),
                net: Some("other".into()),
            };
            project.board.vias.insert(via.id, via);
        }
        let result = query(&project);
        assert_eq!(result.guide_count, 1);
        assert_eq!(result.copper_proof.analyzed_tracks, 0);
        assert_eq!(result.copper_proof.budget_fallback, !invalid);
        assert_eq!(result.copper_proof.fallback_reason, Some(if invalid { "invalid_via_geometry" } else { "hole_candidate_budget" }));
    }
}
