//! Real accessible input drives detached geometry; applying to a component stays explicit.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_engine::EngineHandle;
use viamind_ui_egui::{ViamindApp, actions::UiAction};
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(6);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(6);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().footprint_definition.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().footprint_definition.busy());
}
fn row(h: &mut Harness<'_, ViamindApp>, number: &str) {
    click(h, "Add blank rectangular pad");
    for (label, value) in [
        ("Pad 1 number", number),
        ("Pad 1 X (mm)", "-0.500003"),
        ("Pad 1 Y (mm)", "0.250001"),
        ("Pad 1 width (mm)", "1.200003"),
        ("Pad 1 height (mm)", "0.800001"),
    ] {
        input(h, label, value);
    }
}
#[test]
fn actual_menu_create_odd_nm_save_then_separate_guarded_pad_apply_and_undo() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("definition.json");
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
    let old = engine.read(|p| p.board.parts.values().next().unwrap().clone());
    UiAction::UpdatePart.run(&engine, json!({"id":old.id,"expected":old,"value":"dirty keep"})).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let old = before.project.board.part(&old.id).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(1400.0, 1200.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "Draw wires on canvas");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !h.state().schematic_wires.previewing() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.state().schematic_wires.active());
    click(&mut h, "Libraries");
    click(&mut h, "Footprint definitions…");
    input(&mut h, "Footprint definition name", "Original odd Nm");
    row(&mut h, &old.pads.as_ref().unwrap().0[0].number);
    click(&mut h, "Preview detached footprint");
    let definition = h.state().footprint_definition.preview().unwrap();
    assert_eq!((definition.pads[0].position.x.0, definition.pads[0].width.0, definition.pads[0].height.0), (-500003, 1200003, 800001));
    input(&mut h, "NEW footprint definition path", path.to_str().unwrap());
    click(&mut h, "Save NEW footprint definition");
    wait(&mut h);
    assert_eq!(viamind_doc::footprint::load(&path).unwrap(), h.state().footprint_definition.definition().unwrap());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert!(h.query_by_label_contains("Created ").is_some());
    assert!(h.state().schematic_wires.active(), "detached edits/save retain the live wire draft");
    click(&mut h, "PCB");
    h.state_mut().begin_edit_pads(old.clone());
    h.run_steps(3);
    input(&mut h, "Library file path", path.to_str().unwrap());
    click(&mut h, "Replace pads from library");
    let applied = engine.read(|p| p.board.part(&old.id).unwrap().clone());
    assert_eq!(applied.position, old.position);
    assert_eq!(applied.schematic_symbol, old.schematic_symbol);
    assert_eq!(applied.assembly, old.assembly);
    assert_eq!(applied.pads.as_ref().unwrap().0[0].id, old.pads.as_ref().unwrap().0[0].id);
    assert_eq!(applied.pads.as_ref().unwrap().0[0].net, old.pads.as_ref().unwrap().0[0].net);
    UiAction::Undo.run(&engine, json!({})).unwrap();
    assert_eq!(engine.session_state().project, before.project);
    UiAction::Redo.run(&engine, json!({})).unwrap();
    assert_eq!(engine.read(|p| p.board.part(&old.id).unwrap().clone()), applied);
}
#[test]
fn actual_compact_save_and_error_controls_are_reachable_and_invalid_entry_retained() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("definition.json");
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_definition();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "Footprint definition name", "Compact original");
    row(&mut h, "1");
    input(&mut h, "Pad 1 width (mm)", "0.0000001");
    input(&mut h, "NEW footprint definition path", path.to_str().unwrap());
    h.get_by_label("Save NEW footprint definition").scroll_to_me();
    h.run_steps(8);
    let bounds = h.get_by_label("Save NEW footprint definition").rect();
    assert!(bounds.min.x >= 0.0 && bounds.min.y >= 0.0 && bounds.max.x <= 640.0 && bounds.max.y <= 400.0, "{bounds:?}");
    click(&mut h, "Save NEW footprint definition");
    h.get_by_label_contains("at most six fractional places").scroll_to_me();
    h.run_steps(8);
    let bounds = h.get_by_label_contains("at most six fractional places").rect();
    assert!(bounds.min.y >= 0.0 && bounds.max.y <= 400.0, "{bounds:?}");
    assert!(!path.exists());
    assert!(h.state().footprint_definition.definition().is_err());
    input(&mut h, "Pad 1 width (mm)", "1.200003");
    click(&mut h, "Save NEW footprint definition");
    wait(&mut h);
    assert_eq!(viamind_doc::footprint::load(&path).unwrap().pads[0].width.0, 1200003);
    assert_eq!(engine.session_state(), before);
}
#[test]
fn actual_load_edit_remove_keeps_remaining_row_identity_and_draft_through_peer_and_reopen() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.json");
    let native = dir.path().join("native.json");
    let footprint:viamind_doc::footprint::Footprint=serde_json::from_value(json!({"name":"Original rows","pads":[{"number":"A","position":{"x":-500003,"y":250001},"width":1200003,"height":800001},{"number":"B","position":{"x":750007,"y":-125003},"width":600003,"height":1000001}]})).unwrap();
    viamind_doc::footprint::save_new(&footprint, &source, None).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    UiAction::SaveProject.run(&engine, json!({"path":native,"overwrite":false})).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_definition();
    let mut h = Harness::builder().with_size(vec2(1400.0, 1200.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "Native footprint source path", source.to_str().unwrap());
    click(&mut h, "Load native footprint definition");
    wait(&mut h);
    input(&mut h, "Pad 2 number", "RETAIN");
    let retained_field = h.get_by_label("Pad 2 number").accesskit_node().id();
    click(&mut h, "Remove definition pad 1");
    assert_eq!(h.get_by_label("Pad 1 number").accesskit_node().id(), retained_field);
    assert_eq!(h.state().footprint_definition.definition().unwrap().pads[0].number, "RETAIN");
    assert_eq!(h.state().footprint_definition.definition().unwrap().pads[0].position.x.0, 750007);
    UiAction::PlacePart.run(&engine, json!({"designator":"PEER1","value":"keep","footprint":"old","x_mm":0,"y_mm":0})).unwrap();
    h.run_steps(3);
    let draft = h.state().footprint_definition.definition().unwrap();
    let cursor = engine.event_cursor();
    UiAction::OpenProject.run(&engine, json!({"path":native,"expected_cursor":cursor,"discard_dirty":true})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().footprint_definition.definition().unwrap(), draft);
    assert_eq!(viamind_doc::footprint::load(&source).unwrap(), footprint);
}
