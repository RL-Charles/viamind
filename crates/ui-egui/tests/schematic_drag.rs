//! Actual coordinate, canvas pointer and stale-source connected move controls.
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{ObjectId, schematic::Coordinate};
use viamind_engine::{EngineHandle, Origin};
use viamind_geom::Nm;
use viamind_ui_egui::{ViamindApp, actions::UiAction, camera::WorldPos};
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn fixture() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/connected-drag.viamind.json")).unwrap())
}
fn id(n: u16) -> ObjectId {
    format!("00000000-0000-0000-0000-{n:012x}").parse().unwrap()
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).focus();
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn harness(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(1600.0, 2000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    if h.query_by_label("Symbol U1").is_none() {
        click(&mut h, "Schematic objects");
    }
    click(&mut h, "Symbol U1");
    click(&mut h, "Connected move selected symbol");
    h
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    h.run_steps(3);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.state().schematic_drag.busy() {
        assert!(std::time::Instant::now() < deadline, "connected move worker did not retire");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
    h.run_steps(3);
}
#[test]
fn actual_coordinate_preview_explicit_apply_and_undo_preserve_fixed_objects_and_board() {
    let _serial = SERIAL.lock().unwrap();
    let e = fixture();
    let before = e.session_state();
    let mut h = harness(&e);
    input(&mut h, "Delta X mm", "3");
    input(&mut h, "Delta Y mm", "5");
    click(&mut h, "Preview connected move");
    wait(&mut h);
    assert_eq!(h.state().schematic_drag.report().unwrap().added_wire_count, 3);
    assert_eq!(e.session_state(), before, "preview remains immutable");
    click(&mut h, "Apply connected move");
    wait(&mut h);
    let after = e.session_state();
    let c = after.project.schematic.sheets[&id(502)].content.as_ref().unwrap();
    assert_eq!(c.symbols[&id(503)].position, Coordinate { x: Nm(23_000_000), y: Nm(25_000_000) });
    let old = before.project.schematic.sheets[&id(502)].content.as_ref().unwrap();
    assert_eq!(c.labels, old.labels);
    assert_eq!(c.junctions, old.junctions);
    assert_eq!(c.packages, old.packages);
    for n in [507, 509, 511] {
        assert_eq!(c.symbols[&id(n)], old.symbols[&id(n)]);
    }
    assert_eq!(c.wires.len(), 6);
    assert_eq!(after.project.board, before.project.board);
    assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
    assert!(h.state().schematic_drag.message.as_ref().unwrap().contains("committed"));
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.session_state().project, before.project);
    h.state_mut().redo_design();
    h.run_steps(5);
    assert_eq!(e.session_state().project, after.project);
}
#[test]
fn actual_pointer_origin_grid_release_preview_and_escape_are_local_until_apply() {
    let _serial = SERIAL.lock().unwrap();
    let e = fixture();
    let before = e.session_state();
    let mut h = harness(&e);
    h.state_mut().schematic_drag.open = false;
    h.run_steps(4);
    let cam = h.state().schematic.camera;
    let viewport = h.state().schematic.viewport;
    let start = cam.world_to_screen(WorldPos::new(20.0, 20.0), viewport);
    let end = cam.world_to_screen(WorldPos::new(23.2, 25.3), viewport);
    h.event(Event::PointerMoved(start));
    h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::PointerMoved(end));
    h.run_steps(3);
    h.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    wait(&mut h);
    let report = h.state().schematic_drag.report().expect("release must preview using original press position");
    assert_eq!(report.target_position, Coordinate { x: Nm(23_000_000), y: Nm(25_000_000) });
    assert_eq!(report.added_wire_count, 3);
    assert_eq!(e.session_state(), before);
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    assert!(!h.state().schematic_drag.active());
    assert_eq!(e.session_state(), before);
}
#[test]
fn peer_edit_invalidates_proof_and_invalid_coordinate_never_mutates() {
    let _serial = SERIAL.lock().unwrap();
    let e = fixture();
    let mut h = harness(&e);
    let before = e.session_state();
    input(&mut h, "Delta X mm", "NaN");
    click(&mut h, "Preview connected move");
    assert!(!h.state().schematic_drag.busy());
    assert!(h.state().schematic_drag.report().is_none());
    assert_eq!(e.session_state(), before);
    input(&mut h, "Delta X mm", "3");
    input(&mut h, "Delta Y mm", "5");
    click(&mut h, "Preview connected move");
    wait(&mut h);
    assert!(h.state().schematic_drag.report().is_some());
    UiAction::PlacePart.run(&e, serde_json::json!({"designator":"R9","value":"","footprint":"F","x_mm":0,"y_mm":0})).unwrap();
    let peer = e.session_state();
    h.run_steps(5);
    assert!(h.state().schematic_drag.report().is_none());
    assert!(!h.state().schematic_drag.active());
    assert_eq!(e.session_state(), peer);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.session_state().project, before.project);
}
