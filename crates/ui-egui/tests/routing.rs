//! Real pointer and keyboard events exercise the local routing draft and shared commit.
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::{BoardOutline, Project};
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
    let e = EngineHandle::new(project);
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    (e, h)
}
fn begin(h: &mut Harness<'_, ViamindApp>) {
    h.state_mut().begin_route();
    h.run_steps(5);
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
fn pointer_snap_hover_backspace_escape_are_local_and_enter_commits_one_undo() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10.13, 10.12);
    click(&mut h, 30.1, 10.1);
    assert_eq!(h.state().routing.points(), [p(10_250_000, 10_000_000), p(30_000_000, 10_000_000)]);
    assert_eq!(h.state().routing.hover(), Some(p(30_000_000, 10_000_000)));
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Backspace);
    assert_eq!(h.state().routing.points().len(), 1);
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
    begin(&mut h);
    click(&mut h, 10.13, 10.12);
    click(&mut h, 30.1, 10.1);
    click(&mut h, 30.1, 20.1);
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.read(Clone::clone);
    assert_eq!(after.board.tracks.len(), 2);
    assert!(after.board.tracks.values().all(|t| t.net.as_deref() == Some("GND") && t.width == Nm(250_000)));
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    assert!(h.state().routing.last_commit_ms.unwrap().is_finite());
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn concurrent_agent_change_rejects_whole_route_and_retains_editable_preview() {
    let (e, mut h) = setup();
    begin(&mut h);
    click(&mut h, 10.0, 10.0);
    click(&mut h, 30.0, 10.0);
    e.execute(
        "add_track",
        json!({"start_x_mm":60,"start_y_mm":60,"end_x_mm":70,"end_y_mm":60,"width_mm":0.2,"layer":"top","net":"OTHER"}),
        Origin::agent("concurrent"),
    )
    .unwrap();
    let before = e.session_state();
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), before);
    assert!(h.state().routing.previewing());
    assert!(h.state().routing.error.as_ref().unwrap().contains("session changed"));
    key(&mut h, Key::Backspace);
    assert_eq!(h.state().routing.points().len(), 1);
    key(&mut h, Key::Escape);
    assert_eq!(e.session_state(), before);
}
#[test]
fn routing_blocks_other_editors_and_focus_document_workspace_changes_cancel_locally() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10.0, 10.0);
    h.state_mut().begin_copper();
    h.state_mut().begin_place_part();
    h.state_mut().begin_board_outline();
    h.state_mut().place_test_resistor();
    h.run_steps(3);
    assert!(h.state().routing.previewing());
    assert_eq!(e.session_state(), before);
    assert!(h.query_by_label("Place part").is_none());
    assert!(h.query_by_label("Apply outline").is_none());
    key(&mut h, Key::R);
    key(&mut h, Key::F);
    assert_eq!(e.session_state(), before);
    h.event(Event::WindowFocused(false));
    h.run_steps(2);
    assert!(!h.state().routing.editing());
    h.event(Event::WindowFocused(true));
    h.run_steps(2);
    begin(&mut h);
    h.state_mut().set_schematic_mode(true);
    h.run_steps(2);
    assert!(!h.state().routing.editing());
    h.state_mut().set_schematic_mode(false);
    h.run_steps(3);
    begin(&mut h);
    h.state_mut().begin_document(true);
    h.run_steps(2);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
#[test]
fn commit_clearance_error_keeps_preview_and_does_not_publish_segments() {
    let (e, mut h) = setup();
    e.execute(
        "add_track",
        json!({"start_x_mm":20,"start_y_mm":5,"end_x_mm":20,"end_y_mm":20,"width_mm":0.5,"layer":"top","net":"OTHER"}),
        Origin::agent("obstacle"),
    )
    .unwrap();
    h.run_steps(3);
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10.0, 10.0);
    click(&mut h, 30.0, 10.0);
    key(&mut h, Key::Enter);
    assert!(h.state().routing.previewing());
    assert!(h.state().routing.error.as_ref().unwrap().contains("preflight"));
    assert_eq!(e.session_state(), before);
}
