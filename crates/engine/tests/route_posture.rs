use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::{Command, EngineHandle, Origin, PadEndpointId, RoutePadPosture};
use viamind_geom::{Nm, Point, Polygon};
use viamind_router::{CornerMode, Posture};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> (EngineHandle, [PadEndpointId; 2]) {
    let mut project = Project::new("Exact posture route");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(100_000_000, 0), p(100_000_000, 100_000_000), p(0, 100_000_000)]), vec![]).unwrap());
    let ids = [(10_000_001, 10_000_003), (60_000_007, 30_000_011)].map(|(x, y)| {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: p(123, 257),
            width: Nm(1_000_000),
            height: Nm(1_000_000),
            net: Some("EXACT".into()),
        };
        let part = Part {
            assembly: None,
            id: ObjectId::new(),
            schematic_symbol: None,
            designator: format!("J{x}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, y),
            rotation_mdeg: 90_000,
            side: Side::Top,
            pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
        };
        let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
        project.board.parts.insert(part.id, part);
        id
    });
    (EngineHandle::new(project), ids)
}
fn input(e: &EngineHandle, ids: [PadEndpointId; 2]) -> RoutePadPosture {
    RoutePadPosture {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        start: ids[0],
        target: ids[1],
        waypoints: vec![],
        grid: Nm(250_000),
        width: Nm(250_000),
        mode: CornerMode::Octilinear,
        posture: Posture::Primary,
        rules: Default::default(),
    }
}
#[test]
fn typed_dynamic_postures_keep_exact_stubs_and_one_undo_with_derived_net_side() {
    for (mode, posture, bend) in [
        (CornerMode::Cardinal, Posture::Primary, p(60_000_000, 10_000_000)),
        (CornerMode::Cardinal, Posture::Alternate, p(10_000_000, 30_000_000)),
        (CornerMode::Octilinear, Posture::Primary, p(40_000_000, 10_000_000)),
        (CornerMode::Octilinear, Posture::Alternate, p(30_000_000, 30_000_000)),
    ] {
        for typed in [true, false] {
            let (e, ids) = fixture();
            let before = e.session_state();
            let cmd = RoutePadPosture { mode, posture, ..input(&e, ids) };
            let out = if typed {
                e.execute_typed(cmd, Origin::User)
            } else {
                e.execute(RoutePadPosture::NAME, serde_json::to_value(cmd).unwrap(), Origin::agent("planner"))
            }
            .unwrap();
            let expected = [p(10_000_258, 9_999_880), p(10_000_000, 10_000_000), bend, p(60_000_000, 30_000_000), p(60_000_264, 29_999_888)];
            let after = e.read(Clone::clone);
            assert_eq!(out.created.len(), 4);
            for (id, pair) in out.created.iter().zip(expected.windows(2)) {
                let t = &after.board.tracks[id];
                assert_eq!((t.start, t.end, t.layer, t.net.as_deref()), (pair[0], pair[1], CopperLayer::Top, Some("EXACT")));
            }
            assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
            e.undo(Origin::User).unwrap();
            assert_eq!(e.read(Clone::clone), before.project);
            e.redo(Origin::User).unwrap();
            assert_eq!(e.read(Clone::clone), after);
        }
    }
}
#[test]
fn malformed_waypoints_and_stale_proposal_leave_entire_session_unchanged() {
    let (e, ids) = fixture();
    let cmd = input(&e, ids);
    let before = e.session_state();
    for bad in [
        RoutePadPosture { waypoints: vec![p(1, 1)], ..cmd.clone() },
        RoutePadPosture { waypoints: vec![p(0, 0); 511], ..cmd.clone() },
        RoutePadPosture { grid: Nm(0), ..cmd.clone() },
        RoutePadPosture { width: Nm(0), ..cmd.clone() },
        RoutePadPosture { target: ids[0], ..cmd.clone() },
    ] {
        assert!(e.execute_typed(bad, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
    }
    e.execute("update_part", serde_json::json!({"id":ids[1].part_id,"side":"bottom"}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(cmd, Origin::User).unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), before);
    assert!(e.execute_typed(input(&e, ids), Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
#[test]
fn generated_bend_and_endpoint_stub_both_receive_shared_scoped_preflight() {
    for stub in [false, true] {
        let (e, ids) = fixture();
        // Foreign copper intercepts either the long generated straight leg or the tiny exact attachment stub.
        let (a, b) = if stub { (p(10_000_129, 9_900_000), p(10_000_129, 10_100_000)) } else { (p(25_000_000, 9_000_000), p(25_000_000, 11_000_000)) };
        e.execute("add_track",serde_json::json!({"start_x_mm":a.x.to_mm(),"start_y_mm":a.y.to_mm(),"end_x_mm":b.x.to_mm(),"end_y_mm":b.y.to_mm(),"width_mm":0.25,"layer":"top","net":"FOREIGN"}),Origin::User).unwrap();
        let before = e.session_state();
        assert!(e.execute_typed(input(&e, ids), Origin::User).unwrap_err().to_string().contains("preflight failed"));
        assert_eq!(e.session_state(), before);
    }
}

#[test]
fn exact_named_width_floor_preserves_odd_nm_posture_geometry_without_widening() {
    let (e, ids) = fixture();
    let old = e.session_state();
    e.execute_typed(
        viamind_engine::SetNamedNetRules {
            expected_project: old.project.id,
            expected_cursor: old.cursor,
            rules: vec![viamind_engine::NamedNetRuleRow { net: "EXACT".into(), minimum_copper_clearance: Nm(0), minimum_track_width: Nm(300001) }],
        },
        Origin::User,
    )
    .unwrap();
    let before = e.session_state();
    let request = RoutePadPosture { width: Nm(300000), ..input(&e, ids) };
    assert!(e.execute_typed(request.clone(), Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    e.execute_typed(RoutePadPosture { width: Nm(300001), ..request }, Origin::User).unwrap();
    assert!(e.read(|p| p.board.tracks.values().all(|t| t.width == Nm(300001))));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}
