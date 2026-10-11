use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_engine::{
    Command, CommandCall, EngineHandle, Origin, RouteTerminalId, RouteTerminalPolyline, RouteTerminalPosture, RouteTerminalQuery, TrackEnd,
};
use viamind_geom::{Nm, Point, Polygon};
use viamind_router::{CornerMode, Posture};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> (Project, [RouteTerminalId; 3]) {
    let mut project = Project::new("existing terminals");
    project.board.outline = Some(
        BoardOutline::new(
            Polygon::new(vec![p(-50_000_000, -50_000_000), p(50_000_000, -50_000_000), p(50_000_000, 50_000_000), p(-50_000_000, 50_000_000)]),
            vec![],
        )
        .unwrap(),
    );
    let pad = Pad { id: ObjectId::new(), number: "1".into(), position: p(1, 3), width: Nm(1_000_000), height: Nm(1_000_000), net: Some("N".into()) };
    let part = Part {
        id: ObjectId::new(),
        assembly: None,
        schematic_symbol: None,
        designator: "J1".into(),
        value: String::new(),
        footprint: String::new(),
        position: p(-20_000_000, -15_000_000),
        rotation_mdeg: 90_000,
        side: Side::Top,
        pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
    };
    let pad_id = RouteTerminalId::Pad { part_id: part.id, pad_id: pad.id };
    project.board.parts.insert(part.id, part);
    let t = Track {
        id: ObjectId::new(),
        start: p(-10_000_003, -5_000_001),
        end: p(-5_000_001, -5_000_001),
        width: Nm(350_000),
        layer: CopperLayer::Top,
        net: Some("N".into()),
    };
    let track = RouteTerminalId::TrackEnd { track_id: t.id, end: TrackEnd::End };
    project.board.tracks.insert(t.id, t);
    let v = Via { id: ObjectId::new(), position: p(20_000_003, 15_000_007), diameter: Nm(1_000_000), drill: Nm(300_000), net: Some("N".into()) };
    let via = RouteTerminalId::Via { via_id: v.id };
    project.board.vias.insert(v.id, v);
    (project, [pad_id, track, via])
}
fn manual(e: &EngineHandle, ids: [RouteTerminalId; 3]) -> RouteTerminalPolyline {
    RouteTerminalPolyline {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        start: ids[1],
        target: ids[2],
        layer: CopperLayer::Top,
        interior: vec![p(10_000_000, -5_000_000)],
        grid: Nm(250_000),
        width: Nm(250_000),
        rules: Default::default(),
    }
}
fn posture_input(e: &EngineHandle, ids: [RouteTerminalId; 3], mode: CornerMode, posture: Posture) -> RouteTerminalPosture {
    let i = manual(e, ids);
    RouteTerminalPosture {
        expected_project: i.expected_project,
        expected_cursor: i.expected_cursor,
        start: i.start,
        target: i.target,
        layer: i.layer,
        waypoints: vec![],
        grid: i.grid,
        width: i.width,
        mode,
        posture,
        rules: i.rules,
    }
}
#[test]
fn exact_track_to_via_typed_dynamic_identity_preservation_and_one_undo() {
    for typed in [true, false] {
        let (project, ids) = fixture();
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let input = manual(&e, ids);
        let result = if typed {
            e.execute_typed(input.clone(), Origin::User)
        } else {
            e.execute(RouteTerminalPolyline::NAME, serde_json::to_value(&input).unwrap(), Origin::agent("continuation"))
        }
        .unwrap();
        assert_eq!(result.created.len(), 2);
        let after = e.read(Clone::clone);
        for (id, pair) in result.created.iter().zip([p(-5_000_001, -5_000_001), p(10_000_000, -5_000_000), p(20_000_003, 15_000_007)].windows(2)) {
            let t = &after.board.tracks[id];
            assert_eq!((t.start, t.end, t.width, t.layer, t.net.as_deref()), (pair[0], pair[1], Nm(250_000), CopperLayer::Top, Some("N")));
        }
        for (id, t) in &before.project.board.tracks {
            assert_eq!(after.board.tracks[id], *t);
        }
        assert_eq!(after.board.vias, before.project.board.vias);
        assert_eq!(after.board.parts, before.project.board.parts);
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
    }
}
#[test]
fn independently_expected_all_postures_preserve_negative_off_grid_stubs() {
    for (mode, posture, bend) in [
        (CornerMode::Cardinal, Posture::Primary, p(20_000_000, -5_000_000)),
        (CornerMode::Cardinal, Posture::Alternate, p(-5_000_000, 15_000_000)),
        (CornerMode::Octilinear, Posture::Primary, p(0, -5_000_000)),
        (CornerMode::Octilinear, Posture::Alternate, p(15_000_000, 15_000_000)),
    ] {
        let (project, ids) = fixture();
        let e = EngineHandle::new(project);
        let input = posture_input(&e, ids, mode, posture);
        let result = e.execute_typed(input, Origin::User).unwrap();
        let expected = [p(-5_000_001, -5_000_001), p(-5_000_000, -5_000_000), bend, p(20_000_000, 15_000_000), p(20_000_003, 15_000_007)];
        assert_eq!(result.created.len(), 4);
        for (id, pair) in result.created.iter().zip(expected.windows(2)) {
            let t = e.read(|p| p.board.tracks[id].clone());
            assert_eq!((t.start, t.end), (pair[0], pair[1]));
        }
    }
}
#[test]
fn pads_and_existing_vias_attach_on_both_explicit_faces() {
    for layer in [CopperLayer::Top, CopperLayer::Bottom] {
        let (mut project, ids) = fixture();
        project.board.tracks.clear();
        for part in project.board.parts.values_mut() {
            part.side = if layer == CopperLayer::Top { Side::Top } else { Side::Bottom }
        }
        let e = EngineHandle::new(project);
        let mut input = manual(&e, ids);
        input.start = ids[0];
        input.layer = layer;
        input.interior.clear();
        let result = e.execute_typed(input, Origin::User).unwrap();
        let t = e.read(|p| p.board.tracks[&result.created[0]].clone());
        let expected = if layer == CopperLayer::Top { p(-19_999_997, -15_000_001) } else { p(-19_999_997, -14_999_999) };
        assert_eq!((t.start, t.end, t.layer), (expected, p(20_000_003, 15_000_007), layer));
    }
}
#[test]
fn wrong_layer_net_unassigned_missing_coincident_and_invalid_geometry_are_atomic() {
    for variant in 0..8 {
        let (mut project, ids) = fixture();
        let mut layer = CopperLayer::Top;
        match variant {
            0 => layer = CopperLayer::Bottom,
            1 => project.board.vias.values_mut().next().unwrap().net = Some("n".into()),
            2 => project.board.vias.values_mut().next().unwrap().net = None,
            3 => project.board.vias.clear(),
            4 => project.board.vias.values_mut().next().unwrap().position = p(-5_000_001, -5_000_001),
            5 => project.board.tracks.values_mut().next().unwrap().width = Nm(0),
            6 => project.board.vias.values_mut().next().unwrap().drill = Nm(2_000_000),
            _ => project.board.vias.values_mut().next().unwrap().net = Some("x".repeat(257)),
        }
        let e = EngineHandle::new(project);
        let mut input = manual(&e, ids);
        input.layer = layer;
        let before = e.session_state();
        let status = e.document_status();
        assert!(e.execute_typed(input, Origin::User).is_err(), "variant{variant}");
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
    }
}
#[test]
fn discovery_face_pagination_filters_overflow_unsupported_and_no_mutation() {
    let (mut project, ids) = fixture();
    project.board.parts.values_mut().next().unwrap().rotation_mdeg = 45_000;
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let status = e.document_status();
    let all = e.route_terminal_snapshot().unwrap();
    assert_eq!(all.terminals.len(), 5);
    assert_eq!(all.unsupported_terminals, 1);
    let query = |offset, limit, layer, net| RouteTerminalQuery { offset, limit, layer, net };
    for offset in 0..5 {
        let page = e.query_route_terminals(&query(offset, 1, None, None)).unwrap();
        assert_eq!(page.terminals, vec![all.terminals[offset].clone()]);
        assert_eq!(page.next_offset, (offset < 4).then_some(offset + 1));
    }
    let empty = e.query_route_terminals(&query(usize::MAX, 1000, None, None)).unwrap();
    assert!(empty.terminals.is_empty());
    assert_eq!(empty.next_offset, None);
    let bottom = e.query_route_terminals(&query(0, 100, Some(CopperLayer::Bottom), Some("N".into()))).unwrap();
    assert_eq!(bottom.terminals.len(), 1);
    assert_eq!(bottom.terminals[0].id, ids[2]);
    assert!(e.query_route_terminals(&query(0, 100, None, Some("n".into()))).unwrap().terminals.is_empty());
    for limit in [0, 1001] {
        assert!(e.query_route_terminals(&query(0, limit, None, None)).is_err());
    }
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
#[test]
fn discovery_terminal_row_boundary_precedes_filter_allocation() {
    let mut project = Project::new("bounded endpoints");
    for _ in 0..50_000 {
        let t = Track { id: ObjectId::new(), start: p(0, 0), end: p(1, 0), width: Nm(1), layer: CopperLayer::Top, net: Some("N".into()) };
        project.board.tracks.insert(t.id, t);
    }
    let e = EngineHandle::new(project);
    assert_eq!(e.route_terminal_snapshot().unwrap().terminals.len(), 100_000);
    e.execute(
        "add_track",
        serde_json::json!({"start_x_mm":0,"start_y_mm":0,"end_x_mm":0.000002,"end_y_mm":0,"width_mm":0.000001,"layer":"top","net":"N"}),
        Origin::User,
    )
    .unwrap();
    let before = e.session_state();
    let q = RouteTerminalQuery { net: Some("absent".into()), layer: None, offset: usize::MAX, limit: 1 };
    assert!(e.query_route_terminals(&q).unwrap_err().to_string().contains("100000rows"));
    assert_eq!(e.session_state(), before);
}
#[test]
fn stale_stream_sequence_and_grouped_command_reject_whole_transaction() {
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let stale = manual(&e, ids);
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"x","footprint":"x","x_mm":40,"y_mm":40}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(stale.clone(), Origin::User).is_err());
    let fresh = manual(&e, ids);
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: RouteTerminalPolyline::NAME.into(), input: serde_json::to_value(fresh).unwrap() },
                CommandCall { name: RouteTerminalPolyline::NAME.into(), input: serde_json::to_value(stale).unwrap() }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
    let mut wrong = manual(&e, ids);
    wrong.expected_cursor = EngineHandle::new(Project::new("other stream")).event_cursor();
    assert!(e.execute_typed(wrong, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
#[test]
fn malformed_identity_and_point_budgets_refuse_before_secondary_vectors() {
    for id in [
        serde_json::json!({"kind":"via","via_id":ObjectId::new(),"x":1}),
        serde_json::json!({"kind":"track_end","track_id":ObjectId::new(),"end":"middle"}),
        serde_json::json!({"kind":"pad","part_id":ObjectId::new()}),
    ] {
        assert!(serde_json::from_value::<RouteTerminalId>(id).is_err());
    }
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let mut input = serde_json::to_value(manual(&e, ids)).unwrap();
    input["interior"] = serde_json::to_value(vec![p(0, 0); 1024]).unwrap();
    assert!(e.execute(RouteTerminalPolyline::NAME, input, Origin::User).unwrap_err().to_string().contains("too many route points"));
    let mut input = serde_json::to_value(posture_input(&e, ids, CornerMode::Cardinal, Posture::Primary)).unwrap();
    input["waypoints"] = serde_json::to_value(vec![p(0, 0); 511]).unwrap();
    assert!(e.execute(RouteTerminalPosture::NAME, input, Origin::User).unwrap_err().to_string().contains("too many route points"));
    assert_eq!(e.session_state(), before);
}
#[test]
fn complete_shared_checks_reject_foreign_copper_holes_outline_and_document_width() {
    for variant in 0..4 {
        let (mut project, ids) = fixture();
        match variant {
            0 => {
                let t = Track {
                    id: ObjectId::new(),
                    start: p(0, -8_000_000),
                    end: p(0, -2_000_000),
                    width: Nm(250_000),
                    layer: CopperLayer::Top,
                    net: Some("OTHER".into()),
                };
                project.board.tracks.insert(t.id, t);
            }
            1 => {
                let v =
                    Via { id: ObjectId::new(), position: p(0, -5_000_000), diameter: Nm(1_000_000), drill: Nm(500_000), net: Some("OTHER".into()) };
                project.board.vias.insert(v.id, v);
            }
            2 => project.board.outline = None,
            _ => {
                project.board.net_rules.insert(
                    "N".into(),
                    serde_json::from_value(serde_json::json!({"minimum_copper_clearance":0,"minimum_track_width":250_001})).unwrap(),
                );
            }
        }
        let e = EngineHandle::new(project);
        let mut input = manual(&e, ids);
        input.rules.copper_clearance_mm = 0.0;
        let before = e.session_state();
        let status = e.document_status();
        assert!(e.execute_typed(input, Origin::User).is_err(), "variant{variant}");
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
    }
}
#[test]
fn unused_assembly_and_unrelated_schematic_text_never_enter_terminal_capture_or_route() {
    let (mut project, ids) = fixture();
    let huge = "x".repeat(3 * 1024 * 1024);
    project.board.parts.values_mut().next().unwrap().assembly =
        Some(Box::new(viamind_doc::AssemblyMetadata { manufacturer: Some(huge.clone()), ..Default::default() }));
    project.name = huge.clone();
    let sheet = viamind_doc::Sheet::new(huge.clone());
    project.schematic.sheets.insert(sheet.id, sheet);
    let sheet = viamind_doc::Sheet::new(huge.clone());
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let rows = e.route_terminal_snapshot().unwrap();
    assert_eq!(rows.terminals.len(), 5);
    let result = e.execute_typed(manual(&e, ids), Origin::User).unwrap();
    assert_eq!(result.created.len(), 2);
    assert!(e.read(|p| p.name == huge && p.board.parts.values().next().unwrap().assembly.as_ref().unwrap().manufacturer.as_ref() == Some(&huge)));
}

#[test]
fn borrowed_label_policy_and_incomplete_check_refusals_preserve_full_state() {
    for variant in 0..3 {
        let (mut project, ids) = fixture();
        match variant {
            0 => project.board.parts.values_mut().next().unwrap().designator = "x".repeat(513),
            1 => {
                project.board.net_rules.insert(
                    "x".repeat(257),
                    viamind_doc::NamedNetMinima {
                        minimum_copper_clearance: Nm(0),
                        minimum_track_width: Nm(1),
                        minimum_via_drill: Nm(0),
                        minimum_annular_width: Nm(0),
                    },
                );
            }
            _ => {
                for x in [0, 5_000_000] {
                    let track = Track {
                        id: ObjectId::new(),
                        start: p(x, -8_000_000),
                        end: p(x, -2_000_000),
                        width: Nm(250_000),
                        layer: CopperLayer::Top,
                        net: Some("OTHER".into()),
                    };
                    project.board.tracks.insert(track.id, track);
                }
            }
        }
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let status = e.document_status();
        if variant < 2 {
            assert!(e.route_terminal_snapshot().is_err());
        }
        let mut input = manual(&e, ids);
        input.rules.max_violations = 1;
        let error = e.execute_typed(input, Origin::User).unwrap_err().to_string();
        if variant == 2 {
            assert!(error.contains("truncated"), "{error}");
        }
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
    }
}
