use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
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
    app.begin_pcb_sync();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1200.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Create PCB component from symbol…");
    wait(&mut h, "Schematic symbol for new PCB component");
    h
}
fn choose(h: &mut Harness<'_, ViamindApp>) {
    click(h, "Schematic symbol for new PCB component");
    click(h, "U1 · Power regulator");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/footprints/original-smd3.viamind-footprint.json");
    h.get_by_label("Native footprint library path").focus();
    h.run_steps(3);
    h.get_by_label("Native footprint library path").type_text(&path.to_string_lossy());
    h.run_steps(5);
}
#[test]
fn actual_desktop_selection_path_preview_explicit_placement_creation_and_undo() {
    let e = engine();
    let before = e.session_state();
    let mut h = harness(&e);
    choose(&mut h);
    click(&mut h, "Bottom");
    click(&mut h, "90°");
    click(&mut h, "Prepare new PCB component");
    wait(&mut h, "Create U1");
    assert_eq!(e.session_state(), before, "preparation is read-only");
    assert!(h.query_by_label_contains("3 pads").is_some());
    click(&mut h, "Create PCB component");
    let after = e.read(Clone::clone);
    let part = after.board.parts.values().next().unwrap();
    assert_eq!((part.designator.as_str(), part.side, part.rotation_mdeg), ("U1", viamind_doc::Side::Bottom, 90000));
    assert_eq!((part.position.x.0, part.position.y.0), (40000000, 40000000));
    assert!(part.pads.as_ref().unwrap().0.iter().all(|p| p.net.as_deref().unwrap().starts_with("sc:")));
    assert_eq!(after.schematic, before.project.schematic);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn actual_form_changes_stale_edits_and_same_project_reopen_disable_old_creation() {
    let e = engine();
    let mut h = harness(&e);
    choose(&mut h);
    click(&mut h, "Prepare new PCB component");
    wait(&mut h, "Create U1");
    click(&mut h, "Bottom");
    assert!(h.query_by_label("Create PCB component").is_none(), "form change invalidates preview");
    click(&mut h, "Prepare new PCB component");
    wait(&mut h, "Create U1");
    e.execute(
        "add_part",
        serde_json::json!({"designator":"R99","value":"agent","footprint":"independent","x_mm":5,"y_mm":5}),
        Origin::agent("other"),
    )
    .unwrap();
    h.run_steps(5);
    let before = e.session_state();
    click(&mut h, "Create PCB component");
    assert_eq!(e.session_state(), before);
    assert!(h.query_by_label("Document changed; refresh symbols and prepare again.").is_some());
    click(&mut h, "Refresh symbols");
    wait(&mut h, "Schematic symbol for new PCB component");
    click(&mut h, "Prepare new PCB component");
    wait(&mut h, "Create U1");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("Create PCB component").is_none());
    assert!(h.query_by_label("Schematic symbol for new PCB component").is_none());
}
#[test]
fn actual_invalid_coordinate_and_missing_library_leave_model_unchanged() {
    let e = engine();
    let mut h = harness(&e);
    choose(&mut h);
    let before = e.session_state();
    h.get_by_label("X (integer nm)").focus();
    h.run_steps(3);
    h.get_by_label("X (integer nm)").type_text("garbage");
    h.run_steps(5);
    click(&mut h, "Prepare new PCB component");
    assert!(h.query_by_label("Coordinates must be signed integer nanometres").is_some());
    assert_eq!(e.session_state(), before);
}
