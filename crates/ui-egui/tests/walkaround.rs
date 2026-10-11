//! Real pointer and keyboard events exercise the local routing draft and shared commit.
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::{BoardOutline, ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::{EngineHandle, Origin};
use viamind_geom::{Nm, Point, Polygon};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn setup() -> (EngineHandle, Harness<'static, ViamindApp>) {
    let mut project = Project::new("Pointer routing");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(100_000_000, 0), p(100_000_000, 100_000_000), p(0, 100_000_000)]), vec![]).unwrap());
    for (number, x) in [(1, 10_000_123), (2, 30_000_257), (3, 50_000_000)] {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: Point::ORIGIN,
            width: Nm(1_000_000),
            height: Nm(2_000_000),
            net: Some(if number == 3 { "OTHER" } else { "GND" }.into()),
        };
        let part = Part {
            schematic_symbol: None,
            id: ObjectId::new(),
            designator: format!("J{number}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, if number == 2 { 20_000_129 } else { 10_000_129 }),
            rotation_mdeg: 0,
            side: Side::Top,
            pads: Some(Pads::try_from(vec![pad]).unwrap()),
        };
        project.board.parts.insert(part.id, part);
    }
    let e = EngineHandle::new(project);
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    (e, h)
}
fn begin(h: &mut Harness<'_, ViamindApp>) {
    h.state_mut().begin_pad_route();
    h.run_steps(5);
    h.get_by_label("Static cardinal walkaround").click();
    h.run_steps(3);
    h.get_by_label("Start route").click();
    h.run_steps(5);
    assert!(h.state().routing.previewing());
}
fn click(h: &mut Harness<'_, ViamindApp>, x: f64, y: f64) {
    let pos = h.state().canvas.camera.world_to_screen(WorldPos::new(x, y), h.state().canvas.viewport);
    for pressed in [true, false] {
        h.event(Event::PointerMoved(pos));
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}
fn key(h: &mut Harness<'_, ViamindApp>, key: Key) {
    for pressed in [true, false] {
        h.event(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}
fn wait_plan(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().routing.pad_planned_points().is_empty() {
        h.run_steps(2);
        assert!(h.state().routing.error.is_none(), "{:?}", h.state().routing.error);
        assert!(std::time::Instant::now() < deadline, "walkaround worker timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn actual_fixed_via_checked_preview_replacement_peer_cancel_and_joint_undo() {
    let (original, _) = setup();
    let mut project = original.read(Clone::clone);
    project.board.parts.values_mut().find(|p| p.designator == "J2").unwrap().side = Side::Bottom;
    let e = EngineHandle::new(project);
    for (x, y, yy, layer) in [(15., 8., 12., "top"), (25., 18., 22., "bottom")] {
        e.execute(
            "add_track",
            json!({"start_x_mm":x,"start_y_mm":y,"end_x_mm":x,"end_y_mm":yy,"width_mm":0.25,"layer":layer,"net":"BLOCK"}),
            Origin::agent("fixture"),
        )
        .unwrap();
    }
    let mut h = Harness::builder()
        .with_size(vec2(1600., 1000.))
        .build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    let before = e.session_state();
    let begin = |h: &mut Harness<'_, ViamindApp>| {
        h.state_mut().begin_via_walkaround_route();
        h.run_steps(5);
        h.get_by_label("Start route").click();
        h.run_steps(5);
        click(h, 10., 10.);
        click(h, 30., 20.);
        click(h, 20., 15.);
    };
    begin(&mut h);
    assert_eq!(h.state().routing.pad_via(), Some(p(20_000_000, 15_000_000)));
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), before);
    assert!(h.state().routing.pad_planned_points().is_empty());
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(3);
    wait_plan(&mut h);
    let points = h.state().routing.pad_planned_points();
    assert_eq!(points.first(), Some(&p(10_000_123, 10_000_129)));
    assert_eq!(points.last(), Some(&p(30_000_257, 20_000_129)));
    assert!(points.contains(&p(20_000_000, 15_000_000)));
    assert!(h.query_by_label_contains("Joint checked snapshot").is_some());
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::V);
    assert!(h.state().routing.pad_planned_points().is_empty());
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), before);
    click(&mut h, 21., 15.);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(3);
    wait_plan(&mut h);
    e.execute(
        "add_track",
        json!({"start_x_mm":70.,"start_y_mm":70.,"end_x_mm":75.,"end_y_mm":70.,"width_mm":0.25,"layer":"top","net":"PEER"}),
        Origin::agent("peer"),
    )
    .unwrap();
    let peer = e.session_state();
    h.run_steps(3);
    assert!(h.state().routing.pad_planned_points().is_empty());
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), peer);
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), peer);
    begin(&mut h);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(3);
    wait_plan(&mut h);
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.session_state();
    assert_eq!(after.cursor.sequence, peer.cursor.sequence + 1);
    assert_eq!(after.project.board.vias.len(), peer.project.board.vias.len() + 1);
    let tracks: Vec<_> = after.project.board.tracks.values().filter(|t| t.net.as_deref() == Some("GND")).collect();
    assert!(tracks.iter().any(|t| t.layer == viamind_doc::CopperLayer::Top));
    assert!(tracks.iter().any(|t| t.layer == viamind_doc::CopperLayer::Bottom));
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), peer.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after.project);
}
#[test]
fn actual_static_checkbox_background_plan_cancel_guard_and_atomic_commit() {
    let (e, mut h) = setup();
    e.execute(
        "add_track",
        json!({"start_x_mm":20.,"start_y_mm":0.,"end_x_mm":20.,"end_y_mm":19.,"width_mm":0.25,"layer":"top","net":"BLOCK"}),
        Origin::agent("peer"),
    )
    .unwrap();
    h.run_steps(3);
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    assert!(h.state().routing.pad_planned_points().is_empty());
    key(&mut h, Key::Enter);
    assert!(h.state().routing.previewing());
    assert_eq!(e.session_state(), before);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    let points = h.state().routing.pad_planned_points();
    assert_eq!(points.first(), Some(&p(10_000_123, 10_000_129)));
    assert_eq!(points.last(), Some(&p(30_000_257, 20_000_129)));
    assert!(points.iter().any(|p| p.y.0 >= 19_500_000));
    assert_eq!(e.session_state(), before);
    // Local waypoints are refused, preserving the checked plan and whole session.
    click(&mut h, 25., 30.);
    assert!(h.state().routing.pad_interior().is_empty());
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.read(Clone::clone);
    assert_eq!(after.board.tracks.len(), before.project.board.tracks.len() + points.len() - 1);
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after);
    // Peer changes and same-project replacement invalidate checked previews.
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    e.execute("update_part", json!({"id":e.read(|p|p.board.part_by_designator("J2").unwrap().id),"side":"bottom"}), Origin::agent("peer")).unwrap();
    h.run_steps(3);
    assert!(h.state().routing.pad_planned_points().is_empty());
    let peer = e.session_state();
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), peer);
    key(&mut h, Key::Escape);
}
#[test]
fn actual_cutout_preview_cancel_outline_invalidation_and_one_commit() {
    let (e, mut h) = setup();
    let outline = |top| json!({"outer":[{"x_mm":0.,"y_mm":0.},{"x_mm":100.,"y_mm":0.},{"x_mm":100.,"y_mm":100.},{"x_mm":0.,"y_mm":100.}],"cutouts":[[{"x_mm":19.,"y_mm":top},{"x_mm":21.,"y_mm":top},{"x_mm":21.,"y_mm":22.},{"x_mm":19.,"y_mm":22.}]]});
    let set_outline = |top| {
        e.execute(
            "set_board_outline",
            json!({"expected":{"project_id":e.read(|p|p.id),"outline":e.read(|p|p.board.outline.clone())},"outline":outline(top)}),
            Origin::agent("outline-peer"),
        )
        .unwrap()
    };
    set_outline(8.);
    h.run_steps(3);
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    assert!(h.query_by_label_contains("rectangular outer board and cutouts").is_some());
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    let points = h.state().routing.pad_planned_points();
    assert_eq!(points.first(), Some(&p(10_000_123, 10_000_129)));
    assert_eq!(points.last(), Some(&p(30_000_257, 20_000_129)));
    assert!(points.iter().any(|p| p.y.0 == 7_500_000));
    assert!(points.iter().any(|p| p.x.0 == 21_500_000));
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Escape);
    assert_eq!(e.session_state(), before);
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    set_outline(6.);
    h.run_steps(3);
    assert!(h.state().routing.pad_planned_points().is_empty());
    let peer = e.session_state();
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), peer);
    key(&mut h, Key::Escape);
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    let points = h.state().routing.pad_planned_points();
    assert!(points.iter().any(|p| p.y.0 == 22_500_000), "{points:?}");
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.session_state();
    assert_eq!(after.cursor.sequence, peer.cursor.sequence + 1);
    assert_eq!(after.project.board.tracks.len(), points.len() - 1);
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), peer.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after.project);
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    wait_plan(&mut h);
    let file = tempfile::NamedTempFile::new().unwrap();
    e.read(|p| viamind_doc::file::save(p, file.path())).unwrap();
    let cursor = e.event_cursor();
    e.open_project(
        viamind_engine::OpenProject { path: file.path().to_path_buf(), discard_dirty: true, expected_cursor: Some(cursor) },
        Origin::agent("reopen-peer"),
    )
    .unwrap();
    h.run_steps(3);
    assert!(h.state().routing.pad_planned_points().is_empty());
    let reopened = e.session_state();
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), reopened);
    key(&mut h, Key::Escape);
}
