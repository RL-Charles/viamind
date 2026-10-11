//! Actual pointer/keyboard events through production UI and shared route engine.
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Project, Track, Via};
use viamind_engine::{EngineHandle, Origin, RouteTerminalId, TrackEnd};
use viamind_geom::{Nm, Point, Polygon};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn setup() -> (EngineHandle, Harness<'static, ViamindApp>, [RouteTerminalId; 2]) {
    let mut project = Project::new("UI existing copper");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(100_000_000, 0), p(100_000_000, 100_000_000), p(0, 100_000_000)]), vec![]).unwrap());
    let t = Track {
        id: ObjectId::new(),
        start: p(5_000_001, 10_000_007),
        end: p(10_000_003, 10_000_007),
        width: Nm(500_000),
        layer: CopperLayer::Top,
        net: Some("N".into()),
    };
    let start = RouteTerminalId::TrackEnd { track_id: t.id, end: TrackEnd::End };
    project.board.tracks.insert(t.id, t);
    let v = Via { id: ObjectId::new(), position: p(30_000_001, 25_000_003), diameter: Nm(1_000_000), drill: Nm(300_000), net: Some("N".into()) };
    let target = RouteTerminalId::Via { via_id: v.id };
    project.board.vias.insert(v.id, v);
    let engine = EngineHandle::new(project);
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    (engine, h, [start, target])
}
fn begin(h: &mut Harness<'_, ViamindApp>, mode: Option<&str>) {
    h.state_mut().begin_terminal_route();
    h.run_steps(5);
    if let Some(label) = mode {
        h.get_by_label(label).click();
        h.run_steps(3);
    }
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
fn actual_track_end_via_pick_exact_off_grid_local_interior_and_atomic_finish() {
    let (e, mut h, ids) = setup();
    let before = e.session_state();
    begin(&mut h, None);
    click(&mut h, 10.0, 10.0);
    assert_eq!(h.state().routing.terminal_endpoints(), (Some(ids[0]), None));
    click(&mut h, 30.0, 25.0);
    assert_eq!(h.state().routing.terminal_endpoints(), (Some(ids[0]), Some(ids[1])));
    click(&mut h, 20.13, 20.12);
    assert_eq!(h.state().routing.terminal_interior(), [p(20_250_000, 20_000_000)]);
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Backspace);
    assert!(h.state().routing.terminal_interior().is_empty());
    click(&mut h, 20.13, 20.12);
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.read(Clone::clone);
    assert_eq!(after.board.tracks.len(), 3);
    assert!(after.board.tracks.values().any(|t| t.start == p(10_000_003, 10_000_007) && t.width == Nm(250_000)));
    assert!(after.board.tracks.values().any(|t| t.end == p(30_000_001, 25_000_003)));
    assert_eq!(after.board.vias, before.project.board.vias);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn actual_posture_keyboard_and_button_switches_keep_exact_attachments_and_escape_cancels() {
    let (e, mut h, _) = setup();
    let before = e.session_state();
    begin(&mut h, Some("Cardinal corners"));
    click(&mut h, 10.0, 10.0);
    click(&mut h, 30.0, 25.0);
    let primary =
        vec![p(10_000_003, 10_000_007), p(10_000_000, 10_000_000), p(30_000_000, 10_000_000), p(30_000_000, 25_000_000), p(30_000_001, 25_000_003)];
    let alternate =
        vec![p(10_000_003, 10_000_007), p(10_000_000, 10_000_000), p(10_000_000, 25_000_000), p(30_000_000, 25_000_000), p(30_000_001, 25_000_003)];
    assert_eq!(h.state().routing.terminal_planned_points(), primary);
    key(&mut h, Key::Slash);
    assert_eq!(h.state().routing.terminal_planned_points(), alternate);
    h.get_by_label("Switch terminal posture").click();
    h.run_steps(3);
    assert_eq!(h.state().routing.terminal_planned_points(), primary);
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
#[test]
fn compact_persistent_footer_fits_after_scroll_and_cancel_preserves_full_session() {
    let (e, mut h, _) = setup();
    begin(&mut h, None);
    click(&mut h, 10.0, 10.0);
    click(&mut h, 30.0, 25.0);
    let before = e.session_state();
    h.set_size(vec2(640.0, 400.0));
    h.run_steps(6);
    for label in ["Finish terminal route", "Cancel terminal route"] {
        let r = h.get_by_label(label).rect();
        assert!(r.min.x >= 0.0 && r.min.y >= 0.0 && r.max.x <= 640.0 && r.max.y <= 400.0, "{label}:{r:?}");
    }
    let hover = h.get_by_label("Existing copper route").rect().center();
    h.event(Event::PointerMoved(hover));
    h.event(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -1000.0), modifiers: Modifiers::NONE });
    h.run_steps(5);
    for label in ["Finish terminal route", "Cancel terminal route"] {
        let r = h.get_by_label(label).rect();
        assert!(r.max.x <= 640.0 && r.max.y <= 400.0, "{label}:{r:?}");
    }
    h.get_by_label("Cancel terminal route").click();
    h.run_steps(4);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
#[test]
fn concurrent_edit_invalidates_draft_and_active_modal_prevents_new_picker() {
    let (e, mut h, _) = setup();
    begin(&mut h, None);
    click(&mut h, 10.0, 10.0);
    click(&mut h, 30.0, 25.0);
    h.state_mut().begin_copper();
    assert!(!h.state().copper.editing());
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"x","footprint":"x","x_mm":80,"y_mm":80}), Origin::agent("peer")).unwrap();
    let after = e.session_state();
    h.run_steps(5);
    assert!(!h.state().routing.editing());
    key(&mut h, Key::Enter);
    assert_eq!(e.session_state(), after);
    h.state_mut().begin_copper();
    h.run_steps(3);
    assert!(h.state().copper.editing());
    h.state_mut().begin_terminal_route();
    h.run_steps(3);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), after);
}
#[test]
fn hidden_layer_does_not_pick_terminal_and_focus_loss_cancels_without_edit() {
    let (e, mut h, _) = setup();
    let before = e.session_state();
    begin(&mut h, None);
    h.state_mut().layers.get_mut(viamind_ui_egui::layers::LayerId::F_CU).unwrap().visible = false;
    h.run_steps(3);
    click(&mut h, 10.0, 10.0);
    assert_eq!(h.state().routing.terminal_endpoints(), (None, None));
    h.event(Event::WindowFocused(false));
    h.run_steps(3);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
