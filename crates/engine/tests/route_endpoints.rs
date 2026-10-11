use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::{Command, CommandCall, EngineHandle, Origin, PadEndpointId, RouteEndpointQuery, RoutePadPolyline};
use viamind_geom::{Nm, Point, Polygon};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> (Project, [PadEndpointId; 2]) {
    let mut project = Project::new("Exact pad routing");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(100_000_000, 0), p(100_000_000, 100_000_000), p(0, 100_000_000)]), vec![]).unwrap());
    let ids = [10_000_001, 30_000_007].map(|x| {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: p(123, 257),
            width: Nm(1_000_000),
            height: Nm(1_000_000),
            net: Some("SIGNAL".into()),
        };
        let part = Part {
            artwork: None,
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            id: ObjectId::new(),
            designator: format!("J{x}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, 10_000_003),
            rotation_mdeg: 90_000,
            side: Side::Bottom,
            pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
        };
        let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
        project.board.parts.insert(part.id, part);
        id
    });
    (project, ids)
}
fn input(e: &EngineHandle, ids: [PadEndpointId; 2]) -> RoutePadPolyline {
    RoutePadPolyline {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        start: ids[0],
        target: ids[1],
        interior: vec![p(20_000_000, 20_000_000)],
        grid: Nm(250_000),
        width: Nm(250_000),
        rules: Default::default(),
    }
}
#[test]
fn exact_off_grid_bottom_endpoints_shared_typed_dynamic_and_single_undo() {
    for typed in [true, false] {
        let (project, ids) = fixture();
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let route = input(&e, ids);
        let result = if typed {
            e.execute_typed(route.clone(), Origin::User)
        } else {
            e.execute(RoutePadPolyline::NAME, serde_json::to_value(&route).unwrap(), Origin::agent("pad-router"))
        }
        .unwrap();
        let after = e.read(Clone::clone);
        assert_eq!(result.created.len(), 2);
        let expected = [p(10_000_258, 10_000_126), p(20_000_000, 20_000_000), p(30_000_264, 10_000_126)];
        for (id, pair) in result.created.iter().zip(expected.windows(2)) {
            let track = &after.board.tracks[id];
            assert_eq!((track.start, track.end, track.layer, track.net.as_deref()), (pair[0], pair[1], CopperLayer::Bottom, Some("SIGNAL")));
        }
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
    }
}
#[test]
fn incompatible_missing_arbitrary_and_invalid_interior_reject_atomically() {
    for mutation in 0..7 {
        let (mut project, ids) = fixture();
        let part = project.board.parts.get_mut(&ids[1].part_id).unwrap();
        match mutation {
            0 => part.side = Side::Top,
            1 => part.pads.as_mut().unwrap().0[0].net = Some("OTHER".into()),
            2 => part.pads.as_mut().unwrap().0[0].net = None,
            3 => part.rotation_mdeg = 45_000,
            4 => part.pads = None,
            _ => {}
        }
        let e = EngineHandle::new(project);
        let mut route = input(&e, ids);
        if mutation == 5 {
            route.interior = vec![p(1, 1)];
        }
        if mutation == 6 {
            route.target = route.start;
        }
        let before = e.session_state();
        let status = e.document_status();
        let log = e.with_engine(|e| e.log().len());
        assert!(e.execute_typed(route, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
        assert_eq!(e.with_engine(|e| e.log().len()), log);
    }
}
#[test]
fn stale_stream_sequence_and_batch_fail_without_partial_routes() {
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let old = input(&e, ids);
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"x","footprint":"R_0603","x_mm":80,"y_mm":80}), Origin::agent("concurrent"))
        .unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(old.clone(), Origin::User).is_err());
    assert!(e.execute(RoutePadPolyline::NAME, serde_json::to_value(&old).unwrap(), Origin::User).is_err());
    let mut wrong = input(&e, ids);
    wrong.expected_cursor = EngineHandle::new(Project::new("new stream")).event_cursor();
    assert!(e.execute_typed(wrong, Origin::User).is_err());
    let fresh = input(&e, ids);
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: RoutePadPolyline::NAME.into(), input: serde_json::to_value(fresh).unwrap() },
                CommandCall { name: RoutePadPolyline::NAME.into(), input: serde_json::to_value(old).unwrap() }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
}
#[test]
fn discovery_is_immutable_sorted_filtered_paginated_and_bounded() {
    let (mut project, ids) = fixture();
    project.board.parts.get_mut(&ids[1].part_id).unwrap().rotation_mdeg = 45_000;
    let legacy = Part {
        artwork: None,
        assembly: None,
        schematic_symbol: None,
        schematic_package: None,
        id: ObjectId::new(),
        designator: "LEGACY".into(),
        value: String::new(),
        footprint: String::new(),
        position: p(50_000_000, 50_000_000),
        rotation_mdeg: 0,
        side: Side::Top,
        pads: None,
    };
    project.board.parts.insert(legacy.id, legacy);
    project.schematic.sheets.insert(ObjectId::new(), viamind_doc::Sheet::new("unrelated".repeat(400_000)));
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let snapshot = e.route_endpoint_snapshot().unwrap();
    assert_eq!(snapshot.endpoints.len(), 2);
    assert_eq!(snapshot.guide_endpoints.len(), 1);
    assert_eq!(snapshot.omitted_legacy_parts, 1);
    assert_eq!(snapshot.unsupported_endpoints, 1);
    assert!(snapshot.endpoints.windows(2).all(|w| w[0].id < w[1].id));
    for row in &snapshot.endpoints {
        if row.id == ids[0] {
            assert_eq!(row.center_nm, Some(p(10_000_258, 10_000_126)));
        } else {
            assert!(row.unsupported_reason.as_ref().unwrap().contains("quarter-turn"));
        }
    }
    let query = RouteEndpointQuery { net: Some("SIGNAL".into()), offset: 0, limit: 1 };
    let first = e.query_route_endpoints(&query).unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.next_offset, Some(1));
    assert_eq!(e.query_route_endpoints(&RouteEndpointQuery { offset: 1, ..query.clone() }).unwrap().next_offset, None);
    assert!(e.query_route_endpoints(&RouteEndpointQuery { offset: usize::MAX, ..query.clone() }).unwrap().endpoints.is_empty());
    assert_eq!(e.query_route_endpoints(&RouteEndpointQuery { net: Some("OTHER".into()), ..query.clone() }).unwrap().unsupported_endpoints, 0);
    assert!(e.query_route_endpoints(&RouteEndpointQuery { limit: 0, ..query }).is_err());
    assert_eq!(e.session_state(), before);
    let (mut project, _) = fixture();
    project.board.parts.values_mut().next().unwrap().value = "x".repeat(3 * 1024 * 1024);
    let e = EngineHandle::new(project);
    assert!(e.route_endpoint_snapshot().unwrap_err().to_string().contains("text exceeds"));
}
#[test]
fn foreign_copper_and_incomplete_preflight_reject_all_pad_segments() {
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    e.execute(
        "add_track",
        serde_json::json!({"start_x_mm":20,"start_y_mm":5,"end_x_mm":20,"end_y_mm":25,"width_mm":0.25,"layer":"bottom","net":"OTHER"}),
        Origin::User,
    )
    .unwrap();
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
