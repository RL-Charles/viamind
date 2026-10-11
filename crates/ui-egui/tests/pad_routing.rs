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
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            id: ObjectId::new(),
            designator: format!("J{number}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, 10_000_129),
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
fn pad_picking_retains_exact_off_grid_centers_and_snaps_only_interior_one_undo() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10.0, 10.0);
    let (start, target) = h.state().routing.pad_endpoints();
    assert!(start.is_some());
    assert!(target.is_none());
    click(&mut h, 50.0, 10.0);
    assert!(h.state().routing.error.as_ref().unwrap().contains("same"));
    assert!(h.state().routing.pad_endpoints().1.is_none());
    click(&mut h, 30.0, 10.0);
    assert!(h.state().routing.pad_endpoints().1.is_some());
    click(&mut h, 20.13, 20.12);
    assert_eq!(h.state().routing.pad_interior(), [p(20_250_000, 20_000_000)]);
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Backspace);
    assert!(h.state().routing.pad_interior().is_empty());
    click(&mut h, 20.13, 20.12);
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.read(Clone::clone);
    assert_eq!(after.board.tracks.len(), 2);
    assert!(viamind_ui_egui::board_scene::scene_from_project(&after).ratsnest.is_empty());
    assert!(e.route_endpoint_snapshot().unwrap().guide_endpoints.is_empty());
    assert!(after.board.tracks.values().any(|t| t.start == p(10_000_123, 10_000_129)));
    assert!(after.board.tracks.values().any(|t| t.end == p(30_000_257, 10_000_129)));
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
    assert_eq!(viamind_ui_egui::board_scene::scene_from_project(&before.project).ratsnest.len(), 1);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn guide_click_selects_real_ids_hidden_layers_do_not_intercept_and_escape_cancels() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    h.state_mut().layers.set_visible(viamind_ui_egui::layers::LayerId::F_CU, false);
    h.run_steps(3);
    click(&mut h, 20.0, 10.0);
    assert_eq!(h.state().routing.pad_endpoints(), (None, None));
    h.state_mut().layers.set_visible(viamind_ui_egui::layers::LayerId::F_CU, true);
    h.run_steps(3);
    click(&mut h, 20.0, 10.0);
    let (a, b) = h.state().routing.pad_endpoints();
    assert!(a.is_some() && b.is_some());
    assert_ne!(a, b);
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
#[test]
fn concurrent_pad_edit_rejects_whole_route_and_focus_workspace_cancel_locally() {
    let (e, mut h) = setup();
    begin(&mut h);
    click(&mut h, 20.0, 10.0);
    let part = e.read(|p| p.board.part_by_designator("J1").unwrap().clone());
    e.execute(
        "set_part_pads",
        json!({"id":part.id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":2,"net":"CHANGED"}]}),
        Origin::agent("other"),
    )
    .unwrap();
    let before = e.session_state();
    key(&mut h, Key::Enter);
    assert!(h.state().routing.error.as_ref().unwrap().contains("session changed"));
    assert_eq!(e.session_state(), before);
    assert!(h.state().routing.previewing());
    h.event(Event::WindowFocused(false));
    h.run_steps(2);
    assert!(!h.state().routing.editing());
    h.event(Event::WindowFocused(true));
    h.run_steps(2);
    begin(&mut h);
    h.state_mut().set_schematic_mode(true);
    h.run_steps(2);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}

#[test]
fn arbitrary_angle_pad_refuses_attachment_and_endpoint_buttons_preserve_local_intent() {
    let (e, mut h) = setup();
    let part = e.read(|p| p.board.part_by_designator("J1").unwrap().clone());
    e.execute("update_part", json!({"id":part.id,"rotation_deg":45}), Origin::User).unwrap();
    h.run_steps(3);
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10., 10.);
    assert!(h.state().routing.error.as_ref().unwrap().contains("quarter-turn"));
    assert_eq!(h.state().routing.pad_endpoints(), (None, None));
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Escape);
    let (e, mut h) = setup();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 10.);
    let endpoints = h.state().routing.pad_endpoints();
    click(&mut h, 10., 10.);
    assert_eq!(h.state().routing.pad_endpoints(), endpoints);
    assert!(h.state().routing.pad_interior().is_empty());
    h.get_by_label("Choose target pad").click();
    h.run_steps(3);
    click(&mut h, 30., 10.);
    assert_eq!(h.state().routing.pad_endpoints(), endpoints);
    assert!(e.read(|p| p.board.tracks.is_empty()));
}

#[test]
fn collapsed_tiny_pad_uses_six_point_circular_origin_tolerance() {
    let (e, mut h) = setup();
    let part = e.read(|p| p.board.part_by_designator("J1").unwrap().clone());
    e.execute(
        "set_part_pads",
        json!({"id":part.id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":0.000001,"height_mm":0.000001,"net":"GND"}]}),
        Origin::User,
    )
    .unwrap();
    h.run_steps(3);
    begin(&mut h);
    let offset = 6.5 / h.state().canvas.camera.zoom;
    click(&mut h, 10.000123 + offset, 10.000129 + offset);
    assert!(h.state().routing.pad_endpoints().0.is_none());
    let offset = 5.5 / h.state().canvas.camera.zoom;
    click(&mut h, 10.000123 + offset, 10.000129);
    assert!(h.state().routing.pad_endpoints().0.is_some());
}
