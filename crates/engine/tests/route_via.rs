use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::{Command, CommandCall, EngineHandle, Origin, PadEndpointId, RoutePadViaPolyline};
use viamind_geom::{Nm, Point, Polygon};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> (Project, [PadEndpointId; 2]) {
    let mut project = Project::new("Single through via");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(100_000_000, 0), p(100_000_000, 100_000_000), p(0, 100_000_000)]), vec![]).unwrap());
    let ids = [(10_000_001, Side::Top), (60_000_007, Side::Bottom)].map(|(x, side)| {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: p(123, 257),
            width: Nm(1_000_000),
            height: Nm(1_000_000),
            net: Some("SIGNAL".into()),
            apertures: None,
        };
        let part = Part {
            holes: None,
            artwork: None,
            assembly: None,
            id: ObjectId::new(),
            schematic_symbol: None,
            schematic_package: None,
            designator: format!("J{x}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, 10_000_003),
            rotation_mdeg: 90_000,
            side,
            pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
        };
        let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
        project.board.parts.insert(part.id, part);
        id
    });
    (project, ids)
}
fn input(e: &EngineHandle, ids: [PadEndpointId; 2]) -> RoutePadViaPolyline {
    RoutePadViaPolyline {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        start: ids[0],
        target: ids[1],
        via: p(35_000_000, 20_000_000),
        before: vec![p(20_000_000, 15_000_000)],
        after: vec![p(50_000_000, 15_000_000)],
        grid: Nm(250_000),
        width: Nm(250_000),
        via_diameter: Nm(600_000),
        via_drill: Nm(300_000),
        rules: Default::default(),
    }
}
#[test]
fn exact_opposite_side_legs_and_via_commit_together_typed_dynamic_and_single_undo() {
    for typed in [true, false] {
        let (project, ids) = fixture();
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let route = input(&e, ids);
        let result = if typed {
            e.execute_typed(route.clone(), Origin::User)
        } else {
            e.execute(RoutePadViaPolyline::NAME, serde_json::to_value(route.clone()).unwrap(), Origin::agent("via-router"))
        }
        .unwrap();
        assert_eq!(result.created.len(), 5);
        let after = e.read(Clone::clone);
        let expected = [p(10_000_258, 9_999_880), p(20_000_000, 15_000_000), route.via, p(50_000_000, 15_000_000), p(60_000_264, 10_000_126)];
        for (i, (id, pair)) in result.created.iter().take(4).zip(expected.windows(2)).enumerate() {
            let track = &after.board.tracks[id];
            assert_eq!(
                (track.start, track.end, track.width, track.layer, track.net.as_deref()),
                (pair[0], pair[1], route.width, if i < 2 { CopperLayer::Top } else { CopperLayer::Bottom }, Some("SIGNAL"))
            );
        }
        let via = &after.board.vias[&result.created[4]];
        assert_eq!((via.position, via.diameter, via.drill, via.net.as_deref()), (route.via, route.via_diameter, route.via_drill, Some("SIGNAL")));
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
    }
}
#[test]
fn malformed_grid_dimensions_zero_length_and_via_rules_reject_all_copper() {
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let base = input(&e, ids);
    let before = e.session_state();
    let mut small_drill = base.clone();
    small_drill.rules.minimum_via_drill_mm = 0.4;
    for route in [
        RoutePadViaPolyline { via: p(1, 1), ..base.clone() },
        RoutePadViaPolyline { before: vec![p(1, 1)], ..base.clone() },
        RoutePadViaPolyline { after: vec![base.via], ..base.clone() },
        RoutePadViaPolyline { via_drill: base.via_diameter, ..base.clone() },
        RoutePadViaPolyline { via_drill: Nm(0), ..base.clone() },
        RoutePadViaPolyline { via_diameter: Nm(400_000), ..base.clone() },
        RoutePadViaPolyline { width: Nm(0), ..base.clone() },
        RoutePadViaPolyline { grid: Nm(0), ..base.clone() },
        RoutePadViaPolyline { before: vec![p(20_000_000, 20_000_000); 1023], ..base.clone() },
        small_drill,
    ] {
        assert!(e.execute_typed(route, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
        assert!(!e.document_status().dirty);
    }
}
#[test]
fn incompatible_missing_arbitrary_pads_and_stale_group_reject_atomically() {
    for mutation in 0..4 {
        let (mut project, ids) = fixture();
        let part = project.board.parts.get_mut(&ids[1].part_id).unwrap();
        match mutation {
            0 => part.side = Side::Top,
            1 => part.pads.as_mut().unwrap().0[0].net = Some("OTHER".into()),
            2 => part.rotation_mdeg = 45_000,
            _ => part.pads = None,
        }
        let e = EngineHandle::new(project);
        let before = e.session_state();
        assert!(e.execute_typed(input(&e, ids), Origin::User).is_err());
        assert_eq!(e.session_state(), before);
    }
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let stale = input(&e, ids);
    let part = e.read(|p| p.board.parts[&ids[0].part_id].clone());
    e.execute(
        "set_part_pads",
        serde_json::json!({"id":part.id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":"CHANGED"}]}),
        Origin::agent("concurrent"),
    )
    .unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(stale.clone(), Origin::User).unwrap_err().to_string().contains("session changed"));
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: "add_via".into(), input: serde_json::json!({"x_mm":80,"y_mm":80,"diameter_mm":0.6,"drill_mm":0.3,"net":"OLD"}) },
                CommandCall { name: RoutePadViaPolyline::NAME.into(), input: serde_json::to_value(stale).unwrap() }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
}
#[test]
fn joint_preflight_checks_foreign_copper_both_sides_and_physical_via_holes() {
    for side in ["top", "bottom"] {
        let (project, ids) = fixture();
        let e = EngineHandle::new(project);
        e.execute(
            "add_track",
            serde_json::json!({"start_x_mm":35,"start_y_mm":19,"end_x_mm":35,"end_y_mm":21,"width_mm":0.25,"layer":side,"net":"FOREIGN"}),
            Origin::User,
        )
        .unwrap();
        let before = e.session_state();
        assert!(e.execute_typed(input(&e, ids), Origin::User).unwrap_err().to_string().contains("preflight failed"));
        assert_eq!(e.session_state(), before);
    }
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    e.execute("add_via", serde_json::json!({"x_mm":35.1,"y_mm":20,"diameter_mm":0.6,"drill_mm":0.3,"net":"SIGNAL"}), Origin::User).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(input(&e, ids), Origin::User).unwrap_err().to_string().contains("preflight failed"));
    assert_eq!(e.session_state(), before);
    for y in [110, 120] {
        e.execute(
            "add_track",
            serde_json::json!({"start_x_mm":110,"start_y_mm":y,"end_x_mm":120,"end_y_mm":y,"width_mm":0.25,"layer":"top","net":"OLD"}),
            Origin::User,
        )
        .unwrap();
    }
    let before = e.session_state();
    let mut route = input(&e, ids);
    route.rules.max_violations = 1;
    assert!(e.execute_typed(route, Origin::User).unwrap_err().to_string().contains("incomplete"));
    assert_eq!(e.session_state(), before);
}

#[test]
fn persisted_named_width_floor_checks_both_explicit_via_legs_jointly() {
    let (mut project, ids) = fixture();
    project.board.net_rules.insert(
        "SIGNAL".into(),
        viamind_doc::NamedNetMinima {
            minimum_copper_clearance: Nm(0),
            minimum_track_width: Nm(300001),
            minimum_via_drill: viamind_geom::Nm(0),
            minimum_annular_width: viamind_geom::Nm(0),
        },
    );
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let request = RoutePadViaPolyline { width: Nm(300000), ..input(&e, ids) };
    assert!(e.execute_typed(request.clone(), Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    e.execute_typed(RoutePadViaPolyline { width: Nm(300001), ..request }, Origin::User).unwrap();
    assert!(e.read(|p| p.board.tracks.values().all(|t| t.width == Nm(300001))));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}
#[test]
fn explicit_via_dimensions_refuse_named_drill_and_half_nm_annulus_then_commit_equal() {
    for (drill, diameter, success) in [(300000, 600002, false), (300001, 600002, false), (300001, 600003, true), (300002, 600005, true)] {
        let (mut project, ids) = fixture();
        project.board.net_rules.insert(
            "SIGNAL".into(),
            viamind_doc::NamedNetMinima {
                minimum_copper_clearance: Nm(0),
                minimum_track_width: Nm(1),
                minimum_via_drill: Nm(300001),
                minimum_annular_width: Nm(150001),
            },
        );
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let cmd = RoutePadViaPolyline { via_diameter: Nm(diameter), via_drill: Nm(drill), ..input(&e, ids) };
        let result = e.execute_typed(cmd, Origin::User);
        if success {
            result.unwrap();
            let via = e.read(|p| p.board.vias.values().next().unwrap().clone());
            assert_eq!(via.diameter, Nm(diameter));
            assert_eq!(via.drill, Nm(drill));
            e.undo(Origin::User).unwrap();
            assert_eq!(e.session_state().project, before.project);
        } else {
            assert!(result.unwrap_err().to_string().contains("no resizing"));
            assert_eq!(e.session_state(), before);
        }
    }
}
