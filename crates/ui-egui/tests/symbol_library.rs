use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn wait(h: &mut Harness<'_, ViamindApp>, label: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.query_by_label_contains(label).is_none() {
        assert!(std::time::Instant::now() < end, "missing{label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
}
fn harness(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_symbol_library();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1200.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    wait(&mut h, "Target: Power regulator");
    h
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).focus();
    h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(2);
    h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn load(h: &mut Harness<'_, ViamindApp>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/symbols/original-regulator.viamind-symbol.json");
    input(h, "Symbol library file path", &path.to_string_lossy());
    click(h, "Load symbol template");
    wait(h, "Loaded Original regulator");
}
#[test]
fn actual_library_file_transform_placement_and_undo_preserve_original_instances() {
    let e = engine();
    let before = e.session_state();
    let mut h = harness(&e);
    load(&mut h);
    input(&mut h, "Symbol X (mm)", "40.000001");
    input(&mut h, "Symbol Y (mm)", "50.000003");
    click(&mut h, "90°");
    click(&mut h, "Mirror symbol locally");
    assert_eq!(e.session_state(), before);
    click(&mut h, "Place new library symbol");
    wait(&mut h, "Placed fresh library symbol");
    let after = e.read(Clone::clone);
    let created = after.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().symbols.values().find(|s| s.designator == "U2").unwrap();
    assert_eq!(
        created.pin_position(created.pins[0].position).unwrap(),
        viamind_doc::schematic::Coordinate { x: viamind_geom::Nm(40000001), y: viamind_geom::Nm(46000003) }
    );
    assert_eq!(after.board, before.project.board);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn actual_invalid_coordinates_peer_edit_and_reopened_context_never_place_old_template() {
    let e = engine();
    let mut h = harness(&e);
    load(&mut h);
    let before = e.session_state();
    input(&mut h, "Symbol X (mm)", "1.0000001");
    click(&mut h, "Place new library symbol");
    wait(&mut h, "Coordinates require decimal mm");
    assert_eq!(e.session_state(), before);
    input(&mut h, "Symbol X (mm)", "40");
    e.execute("add_part", serde_json::json!({"designator":"R99","value":"peer","footprint":"F","x_mm":1,"y_mm":2}), Origin::agent("peer")).unwrap();
    h.run_steps(5);
    let peer = e.session_state();
    click(&mut h, "Place new library symbol");
    assert_eq!(e.session_state(), peer);
    assert!(h.query_by_label("Document changed; refresh the library target sheet before placement or export.").is_some());
    click(&mut h, "Refresh library target sheet");
    wait(&mut h, "Target: Power regulator");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    viamind_ui_egui::actions::UiAction::OpenProject
        .run(&e, serde_json::json!({"path":path,"discard_dirty":false,"expected_cursor":e.event_cursor()}))
        .unwrap();
    h.run_steps(5);
    assert!(h.query_by_label_contains("Loaded Original regulator").is_none());
    assert_eq!(e.read(|p| p.schematic.clone()), peer.project.schematic);
}
#[test]
fn actual_new_export_and_collision_use_detached_writer_without_session_changes() {
    let e = engine();
    let before = e.session_state();
    let mut h = harness(&e);
    click(&mut h, "Library source symbol");
    click(&mut h, "U1");
    input(&mut h, "Library template name", "Original regulator");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("symbol.json");
    input(&mut h, "Symbol library file path", &path.to_string_lossy());
    click(&mut h, "Export new symbol template file");
    wait(&mut h, "Created ");
    let bytes = std::fs::read(&path).unwrap();
    let template = viamind_doc::symbol_library::from_slice(&bytes).unwrap();
    assert_eq!(template.pins.len(), 3);
    assert_eq!(template.name, "Original regulator");
    assert_eq!(e.session_state(), before);
    click(&mut h, "Export new symbol template file");
    wait(&mut h, "exist");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(e.session_state(), before);
}

#[test]
fn compact_viewport_reaches_export_and_error_results_by_scrolling() {
    let e = engine();
    let before = e.session_state();
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_symbol_library();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    wait(&mut h, "Target: Power regulator");
    input(&mut h, "Symbol library file path", "/tmp/viamind-t0080-missing-parent/export.json");
    h.get_by_label("Export new symbol template file").scroll_to_me();
    h.run_steps(10);
    let rect = h.get_by_label("Export new symbol template file").rect();
    assert!(rect.min.y >= 0.0 && rect.max.y <= 400.0 && rect.min.x >= 0.0 && rect.max.x <= 640.0, "{rect:?}");
    click(&mut h, "Export new symbol template file");
    wait(&mut h, "No such file");
    h.get_by_label_contains("No such file").scroll_to_me();
    h.run_steps(10);
    let rect = h.get_by_label_contains("No such file").rect();
    assert!(rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    assert_eq!(e.session_state(), before);
}
