use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1800.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
}
#[test]
fn actual_forms_create_all_schematic_types_edit_and_undo_without_board_changes() {
    let engine = EngineHandle::new(Project::new("Desktop schematic"));
    let mut h = harness(&engine);
    input(&mut h, "Sheet name", "Power");
    click(&mut h, "Create schematic sheet");
    click(&mut h, "Add symbol");
    input(&mut h, "Reference", "U7");
    input(&mut h, "Symbol value", "Regulator");
    input(&mut h, "Schematic X (mm)", "12.000001");
    input(&mut h, "Schematic Y (mm)", "20.000002");
    input(&mut h, "Pin 1 name", "VIN");
    click(&mut h, "Add pin");
    input(&mut h, "Pin 2 name", "GND");
    input(&mut h, "Pin 2 X (mm)", "3");
    click(&mut h, "Apply schematic object");
    let sheet = engine.read(|p| p.schematic.sheets.values().next().unwrap().clone());
    let s = sheet.content.as_ref().unwrap().symbols.values().next().unwrap();
    let id = s.id;
    let pin = s.pins[0].id;
    assert_eq!(s.position.x.0, 12000001);
    assert_eq!(s.pins.len(), 2);
    click(&mut h, "Add wire");
    input(&mut h, "Schematic X (mm)", "-1.234567");
    input(&mut h, "Wire end X (mm)", "0.000001");
    click(&mut h, "Apply schematic object");
    click(&mut h, "Add junction");
    click(&mut h, "Apply schematic object");
    click(&mut h, "Add label");
    input(&mut h, "Label name", "VIN");
    click(&mut h, "Power port");
    click(&mut h, "Apply schematic object");
    let all = engine.read(Clone::clone);
    let c = all.schematic.sheets[&sheet.id].content.as_ref().unwrap();
    assert_eq!((c.symbols.len(), c.wires.len(), c.junctions.len(), c.labels.len()), (1, 1, 1, 1));
    assert_eq!(c.wires.values().next().unwrap().start.x.0, -1234567);
    assert_eq!(c.labels.values().next().unwrap().scope, viamind_doc::schematic::LabelScope::Power);
    assert!(all.board.parts.is_empty());
    click(&mut h, "Schematic objects");
    click(&mut h, "Symbol U7");
    input(&mut h, "Symbol value", "Updated regulator");
    click(&mut h, "Apply schematic object");
    assert_eq!(engine.read(|p| p.schematic.sheets[&sheet.id].content.as_ref().unwrap().symbols[&id].pins[0].id), pin);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(engine.read(Clone::clone), all);
    click(&mut h, "Symbol U7");
    click(&mut h, "Delete schematic object");
    assert!(engine.read(|p| p.schematic.sheets[&sheet.id].content.as_ref().unwrap().symbols.is_empty()));
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(engine.read(Clone::clone), all);
    let camera = h.state().schematic.camera;
    click(&mut h, "PCB");
    assert!(!h.state().schematic.active);
    click(&mut h, "Schematic");
    assert!(h.state().schematic.active);
    assert_eq!(h.state().schematic.camera, camera);
}
#[test]
fn invalid_and_stale_drafts_preserve_every_session_field_until_explicit_reload() {
    let engine = EngineHandle::new(Project::new("Stale schematic"));
    let mut h = harness(&engine);
    click(&mut h, "Create schematic sheet");
    click(&mut h, "Add wire");
    input(&mut h, "Wire end X (mm)", "0.0000001");
    let before = engine.session_state();
    click(&mut h, "Apply schematic object");
    assert_eq!(engine.session_state(), before);
    assert!(h.query_by_label("Apply schematic object").is_some());
    click(&mut h, "Cancel schematic draft");
    click(&mut h, "Add junction");
    let sheet = engine.read(|p| p.schematic.sheets.values().next().unwrap().clone());
    engine
        .execute(
            "edit_schematic_sheet",
            json!({"expected_project":before.project.id,"sheet_id":sheet.id,"expected":sheet,"name":"Agent renamed","edits":[]}),
            Origin::agent("schematic-peer"),
        )
        .unwrap();
    h.run_steps(5);
    let after = engine.session_state();
    click(&mut h, "Apply schematic object");
    assert_eq!(engine.session_state(), after);
    click(&mut h, "Rename sheet");
    assert_eq!(engine.session_state(), after, "sheet details keep their own stale guard");
    click(&mut h, "Reload schematic draft");
    click(&mut h, "Apply schematic object");
    assert_eq!(engine.read(|p| p.schematic.sheets[&sheet.id].name.clone()), "Agent renamed");
    assert_eq!(engine.read(|p| p.schematic.sheets[&sheet.id].content.as_ref().unwrap().junctions.len()), 1);
}
#[test]
fn reopen_same_project_resets_old_draft_using_session_stream() {
    let engine = EngineHandle::new(Project::new("Reload"));
    let mut h = harness(&engine);
    click(&mut h, "Create schematic sheet");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.viamind.json");
    engine.save_project(viamind_engine::SaveProject { path: Some(path.clone()), ..Default::default() }).unwrap();
    click(&mut h, "Add label");
    input(&mut h, "Label name", "Old draft");
    let cursor = engine.session_state().cursor;
    engine.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: Some(cursor) }, Origin::agent("loader")).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("Apply schematic object").is_none());
    assert!(engine.read(|p| p.schematic.sheets.values().all(|s| s.content.as_ref().unwrap().labels.is_empty())));
}

#[test]
fn view_fit_and_status_follow_the_visible_schematic_camera() {
    let engine = EngineHandle::new(Project::new("Fit"));
    let mut h = harness(&engine);
    click(&mut h, "Create schematic sheet");
    click(&mut h, "Add junction");
    input(&mut h, "Schematic X (mm)", "123");
    input(&mut h, "Schematic Y (mm)", "456");
    click(&mut h, "Apply schematic object");
    h.state_mut().schematic.camera.center = viamind_ui_egui::camera::WorldPos::new(999.0, 999.0);
    h.state_mut().schematic.camera.zoom = 1.0;
    let board_camera = h.state().canvas.camera;
    click(&mut h, "View");
    click(&mut h, "Zoom to fit (Home)");
    assert_eq!(h.state().schematic.camera.center, viamind_ui_egui::camera::WorldPos::new(123.0, 456.0));
    assert_eq!(h.state().canvas.camera, board_camera);
}
