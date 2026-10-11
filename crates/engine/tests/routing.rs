use serde_json::json;
use viamind_doc::{BoardOutline, CopperLayer, Project};
use viamind_engine::{Command, CommandCall, CommandRegistry, EngineHandle, Origin, RoutePolyline};
use viamind_geom::{Nm, Point, Polygon};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 1_000_000), Nm(y * 1_000_000))
}
fn ring(a: i64, b: i64) -> Polygon {
    Polygon::new(vec![p(a, a), p(b, a), p(b, b), p(a, b)])
}
fn setup() -> EngineHandle {
    let mut project = Project::new("Route");
    project.board.outline = Some(BoardOutline::new(ring(-100, 100), vec![]).unwrap());
    EngineHandle::new(project)
}
fn route(e: &EngineHandle) -> RoutePolyline {
    let state = e.session_state();
    RoutePolyline {
        expected_project: state.project.id,
        expected_cursor: state.cursor,
        points: vec![p(10, 10), p(30, 10), p(30, 20)],
        grid: Nm(250_000),
        width: Nm(250_000),
        layer: CopperLayer::Top,
        net: "GND".into(),
        rules: Default::default(),
    }
}
#[test]
fn typed_and_dynamic_routes_preserve_exact_points_ids_and_one_undo_redo() {
    for typed in [true, false] {
        let e = setup();
        let before = e.session_state();
        let input = route(&e);
        let result = if typed {
            e.execute_typed(input.clone(), Origin::User)
        } else {
            e.execute(RoutePolyline::NAME, serde_json::to_value(&input).unwrap(), Origin::agent("router"))
        }
        .unwrap();
        assert_eq!(result.created.len(), 2);
        let after = e.read(Clone::clone);
        for (id, pair) in result.created.iter().zip(input.points.windows(2)) {
            let track = &after.board.tracks[id];
            assert_eq!(
                (track.start, track.end, track.width, track.layer, track.net.as_deref()),
                (pair[0], pair[1], input.width, input.layer, Some("GND"))
            );
        }
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
    }
}
#[test]
fn invalid_points_and_stale_stream_sequence_reject_without_state_history_events() {
    let e = setup();
    let input = route(&e);
    e.execute("add_part", json!({"designator":"R1","value":"1k","footprint":"R_0603","x_mm":50,"y_mm":50}), Origin::agent("other")).unwrap();
    let before = e.session_state();
    let status = e.document_status();
    let log = e.with_engine(|x| x.log().len());
    assert!(e.execute_typed(input.clone(), Origin::User).is_err());
    assert!(e.execute(RoutePolyline::NAME, serde_json::to_value(input).unwrap(), Origin::User).is_err());
    let fresh = route(&e);
    for input in [
        RoutePolyline { expected_cursor: setup().event_cursor(), ..fresh.clone() },
        RoutePolyline { expected_project: Project::new("other").id, ..fresh.clone() },
        RoutePolyline { points: vec![p(10, 10), p(10, 10)], ..fresh.clone() },
        RoutePolyline { points: vec![Point::new(Nm(1), Nm(1)), p(20, 20)], ..fresh.clone() },
        RoutePolyline { points: vec![p(10, 10)], ..fresh.clone() },
        RoutePolyline { width: Nm(0), ..fresh.clone() },
        RoutePolyline { grid: Nm(0), ..fresh.clone() },
        RoutePolyline { net: " GND".into(), ..fresh.clone() },
        RoutePolyline { net: String::new(), ..fresh.clone() },
        RoutePolyline { points: vec![p(10, 10), p(10001, 10)], ..fresh.clone() },
    ] {
        assert!(e.execute(RoutePolyline::NAME, serde_json::to_value(input).unwrap(), Origin::User).is_err());
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
        assert_eq!(e.with_engine(|x| x.log().len()), log);
    }
    let mut project = before.project.clone();
    assert!(
        CommandRegistry::builtin().dispatch(RoutePolyline::NAME, serde_json::to_value(fresh).unwrap(), &mut project).is_err(),
        "required cursor commands need a live transaction context"
    );
    assert_eq!(project, before.project);
}
#[test]
fn batch_uses_one_captured_cursor_and_rejects_stale_batches_atomically() {
    let e = setup();
    let before = e.session_state();
    let a = route(&e);
    let b = RoutePolyline { points: vec![p(30, 20), p(40, 20), p(40, 30)], ..a.clone() };
    let calls = vec![
        CommandCall { name: RoutePolyline::NAME.into(), input: serde_json::to_value(&a).unwrap() },
        CommandCall { name: RoutePolyline::NAME.into(), input: serde_json::to_value(&b).unwrap() },
    ];
    e.execute_batch(calls.clone(), Origin::User, None).unwrap();
    assert_eq!(e.read(|p| p.board.tracks.len()), 4);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    let checkpoint = e.session_state();
    let error = e
        .execute_batch(
            vec![
                CommandCall {
                    name: "add_track".into(),
                    input: json!({"start_x_mm":1,"start_y_mm":1,"end_x_mm":2,"end_y_mm":2,"width_mm":0.25,"layer":"top","net":"GND"}),
                },
                calls[0].clone(),
            ],
            Origin::User,
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("session changed"));
    assert_eq!(e.session_state(), checkpoint, "earlier batch steps roll back without publishing");
}
#[test]
fn preflight_rejects_foreign_copper_and_cutouts_but_preserves_existing_unrelated_violations() {
    let e = setup();
    e.execute(
        "add_track",
        json!({"start_x_mm":20,"start_y_mm":0,"end_x_mm":20,"end_y_mm":30,"width_mm":0.25,"layer":"top","net":"VCC"}),
        Origin::User,
    )
    .unwrap();
    let checkpoint = e.session_state();
    assert!(e.execute_typed(route(&e), Origin::User).unwrap_err().to_string().contains("preflight failed"));
    assert_eq!(e.session_state(), checkpoint);
    let mut project = Project::new("Cutout");
    project.board.outline = Some(BoardOutline::new(ring(-100, 100), vec![ring(10, 20)]).unwrap());
    let e = EngineHandle::new(project);
    let mut input = route(&e);
    input.points = vec![p(5, 15), p(25, 15)];
    let checkpoint = e.session_state();
    assert!(e.execute_typed(input, Origin::User).is_err());
    assert_eq!(e.session_state(), checkpoint);
    let e = setup();
    e.execute(
        "add_track",
        json!({"start_x_mm":120,"start_y_mm":0,"end_x_mm":130,"end_y_mm":0,"width_mm":0.25,"layer":"top","net":"VCC"}),
        Origin::User,
    )
    .unwrap();
    e.execute_typed(route(&e), Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.tracks.len()), 3, "unrelated old violations are explicitly permitted");
    e.execute(
        "add_track",
        json!({"start_x_mm":-110,"start_y_mm":-20,"end_x_mm":-120,"end_y_mm":-20,"width_mm":0.25,"layer":"bottom","net":"OLD"}),
        Origin::User,
    )
    .unwrap();
    let mut input = route(&e);
    input.rules.max_violations = 1;
    let checkpoint = e.session_state();
    assert!(e.execute_typed(input, Origin::User).unwrap_err().to_string().contains("preflight incomplete"));
    assert_eq!(e.session_state(), checkpoint);
    let e = EngineHandle::new(Project::new("Missing outline"));
    let checkpoint = e.session_state();
    assert!(e.execute_typed(route(&e), Origin::User).is_err());
    assert_eq!(e.session_state(), checkpoint);
}

#[test]
fn bounded_board_capture_ignores_large_unrelated_schematic_and_rejects_oversized_board_text() {
    let mut project = Project::new("x".repeat(3 * 1024 * 1024));
    project.board.outline = Some(BoardOutline::new(ring(-100, 100), vec![]).unwrap());
    let sheet = viamind_doc::Sheet::new("unrelated".repeat(400_000));
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let input = route(&e);
    e.execute_typed(input, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.schematic.sheets.values().next().unwrap().name.len()), "unrelated".len() * 400_000);
    assert_eq!(e.read(|p| p.name.len()), 3 * 1024 * 1024);
    let mut board = e.read(|p| p.board.clone());
    let part = viamind_doc::Part {
        schematic_symbol: None,
        id: viamind_doc::ObjectId::new(),
        designator: "R1".into(),
        value: "x".repeat(3 * 1024 * 1024),
        footprint: "R_0603".into(),
        position: p(50, 50),
        rotation_mdeg: 0,
        side: viamind_doc::Side::Top,
        pads: None,
    };
    board.parts.insert(part.id, part);
    let mut project = Project::new("Oversized board");
    project.board = board;
    let e = EngineHandle::new(project);
    let before = e.session_state();
    assert!(e.execute_typed(route(&e), Origin::User).unwrap_err().to_string().contains("snapshot text exceeds"));
    assert_eq!(e.session_state(), before);
    let e = setup();
    let before = e.session_state();
    let mut input = route(&e);
    input.net = "N".repeat(3 * 1024 * 1024);
    assert!(e.execute_typed(input, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
