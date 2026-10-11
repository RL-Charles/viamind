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
            position: p(x, 10_000_129),
            rotation_mdeg: 0,
            side: if number == 2 { Side::Bottom } else { Side::Top },
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
    h.state_mut().begin_via_route();
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
fn local_via_and_both_legs_snap_then_commit_as_one_undo() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 30., 10.);
    assert!(h.state().routing.pad_via().is_none());
    key(&mut h, Key::Enter);
    assert!(h.state().routing.error.as_ref().unwrap().contains("Place"));
    assert_eq!(e.session_state(), before);
    click(&mut h, 20.13, 20.12);
    assert_eq!(h.state().routing.pad_via(), Some(p(20_250_000, 20_000_000)));
    click(&mut h, 15.12, 15.13);
    assert_eq!(h.state().routing.pad_interior(), [p(15_000_000, 15_250_000)]);
    h.get_by_label("Edit target leg").click();
    h.run_steps(3);
    click(&mut h, 25.12, 15.13);
    assert_eq!(h.state().routing.pad_after(), [p(25_000_000, 15_250_000)]);
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Backspace);
    assert!(h.state().routing.pad_after().is_empty());
    click(&mut h, 25.12, 15.13);
    key(&mut h, Key::V);
    click(&mut h, 20.12, 22.13);
    assert_eq!(h.state().routing.pad_via(), Some(p(20_000_000, 22_250_000)));
    assert_eq!(h.state().routing.pad_interior().len(), 1);
    assert_eq!(h.state().routing.pad_after().len(), 1);
    key(&mut h, Key::Enter);
    assert!(!h.state().routing.editing(), "{:?}", h.state().routing.error);
    let after = e.read(Clone::clone);
    assert_eq!((after.board.tracks.len(), after.board.vias.len()), (4, 1));
    assert!(after.board.tracks.values().any(|t| t.start == p(10_000_123, 10_000_129)));
    assert!(after.board.tracks.values().any(|t| t.end == p(30_000_257, 10_000_129)));
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn endpoint_changes_clear_geometry_and_cancel_never_mutates() {
    let (e, mut h) = setup();
    let before = e.session_state();
    begin(&mut h);
    click(&mut h, 10., 10.);
    click(&mut h, 50., 10.);
    assert!(h.state().routing.error.as_ref().unwrap().contains("net"));
    assert!(h.state().routing.pad_endpoints().1.is_none());
    click(&mut h, 30., 10.);
    click(&mut h, 20., 20.);
    click(&mut h, 15., 15.);
    h.get_by_label("Choose target pad").click();
    h.run_steps(3);
    click(&mut h, 30., 10.);
    assert!(h.state().routing.pad_via().is_none());
    assert!(h.state().routing.pad_interior().is_empty());
    click(&mut h, 20., 20.);
    key(&mut h, Key::V);
    key(&mut h, Key::Backspace);
    assert!(h.state().routing.pad_via().is_none());
    key(&mut h, Key::Escape);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
#[test]
fn guide_requires_both_visible_sides_and_concurrent_edit_rejects_all_copper() {
    let (e, mut h) = setup();
    begin(&mut h);
    h.state_mut().layers.set_visible(viamind_ui_egui::layers::LayerId::B_CU, false);
    h.run_steps(3);
    click(&mut h, 20., 10.);
    assert_eq!(h.state().routing.pad_endpoints(), (None, None));
    h.state_mut().layers.set_visible(viamind_ui_egui::layers::LayerId::B_CU, true);
    h.run_steps(3);
    click(&mut h, 20., 10.);
    let (a, b) = h.state().routing.pad_endpoints();
    assert!(a.is_some() && b.is_some());
    click(&mut h, 20., 20.);
    let part = e.read(|p| p.board.part_by_designator("J1").unwrap().clone());
    e.execute("update_part", json!({"id":part.id,"side":"bottom"}), Origin::agent("other")).unwrap();
    let before = e.session_state();
    key(&mut h, Key::Enter);
    assert!(h.state().routing.error.as_ref().unwrap().contains("session changed"));
    assert_eq!(e.session_state(), before);
    assert!(h.state().routing.previewing());
    h.event(Event::WindowFocused(false));
    h.run_steps(2);
    assert!(!h.state().routing.editing());
    assert_eq!(e.session_state(), before);
}
