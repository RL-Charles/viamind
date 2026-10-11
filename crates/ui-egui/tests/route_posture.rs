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
            artwork: None,
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
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
    h.get_by_label("45-degree corners").click();
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
#[test]
fn actual_posture_toggle_replans_exact_stubs_locally_then_commits_one_undo() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    assert_eq!(
        h.state().routing.pad_planned_points(),
        [p(10_000_123, 10_000_129), p(10_000_000, 10_000_000), p(20_000_000, 10_000_000), p(30_000_000, 20_000_000), p(30_000_257, 20_000_129)]
    );
    h.get_by_label("Posture: Straight first").click();
    h.run_steps(3);
    assert_eq!(h.state().routing.pad_planned_points()[2], p(20_000_000, 20_000_000));
    assert_eq!(e.session_state(), before);
    click(&mut h, 20.13, 25.12);
    assert_eq!(h.state().routing.pad_interior(), [p(20_250_000, 25_000_000)]);
    assert!(h.state().routing.pad_planned_points().contains(&p(20_250_000, 25_000_000)));
    key(&mut h, Key::Backspace);
    assert!(h.state().routing.pad_interior().is_empty());
    let points = h.state().routing.pad_planned_points();
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.read(Clone::clone);
    assert_eq!(after.board.tracks.len(), points.len() - 1);
    for pair in points.windows(2) {
        assert!(after.board.tracks.values().any(|t| t.start == pair[0] && t.end == pair[1]));
    }
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn actual_cardinal_choice_and_escape_cancel_without_design_events() {
    let (e, mut h) = setup();
    let before = e.session_state();
    h.state_mut().begin_pad_route();
    h.run_steps(3);
    h.get_by_label("Cardinal corners").click();
    h.run_steps(3);
    h.get_by_label("Start route").click();
    h.run_steps(3);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    assert_eq!(h.state().routing.pad_planned_points()[2], p(30_000_000, 10_000_000));
    h.get_by_label("Posture: Horizontal first").click();
    h.run_steps(3);
    assert_eq!(h.state().routing.pad_planned_points()[2], p(10_000_000, 20_000_000));
    click(&mut h, 20., 25.);
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
#[test]
fn stale_peer_edit_rejects_generated_path_and_preserves_editable_local_waypoints() {
    let (e, mut h) = setup();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 20.);
    click(&mut h, 20., 25.);
    let part = e.read(|p| p.board.part_by_designator("J1").unwrap().id);
    e.execute("update_part", json!({"id":part,"side":"bottom"}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    key(&mut h, Key::Enter);
    assert!(h.state().routing.error.as_ref().unwrap().contains("session changed"));
    assert!(h.state().routing.previewing());
    assert_eq!(h.state().routing.pad_interior(), [p(20_000_000, 25_000_000)]);
    assert_eq!(e.session_state(), before);
}
