use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap())
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn settle(h: &mut Harness<'_, ViamindApp>, label: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.query_by_label_contains(label).is_none() {
        assert!(std::time::Instant::now() < deadline, "missing {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
}
fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_pcb_sync();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1200.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h
}
#[test]
fn actual_desktop_preview_apply_and_one_undo_updates_pads_and_preserves_schematic() {
    let e = engine();
    let before = e.read(Clone::clone);
    let mut h = harness(&e);
    // Wait for initial link capture before scheduling the single preview worker.
    click(&mut h, "Component links");
    settle(&mut h, "PCB component to link");
    click(&mut h, "Prepare synchronization");
    settle(&mut h, "13 proposed changes");
    assert_eq!(e.read(Clone::clone), before, "preview is read-only");
    click(&mut h, "Apply synchronization");
    let after = e.read(Clone::clone);
    assert_eq!(after.schematic, before.schematic);
    assert_ne!(after.board, before.board);
    assert!(after.board.parts.values().all(|p| p.pads.as_ref().unwrap().0.iter().all(|p| p.net.is_some())));
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), before);
    h.state_mut().redo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn stale_and_same_uuid_reopened_desktop_previews_cannot_apply() {
    let e = engine();
    let mut h = harness(&e);
    click(&mut h, "Component links");
    settle(&mut h, "PCB component to link");
    click(&mut h, "Prepare synchronization");
    settle(&mut h, "13 proposed changes");
    let id = e.read(|p| *p.board.parts.keys().next().unwrap());
    e.execute("update_part", serde_json::json!({"id":id,"value":"agent edit"}), Origin::agent("other")).unwrap();
    h.run_steps(5);
    let checkpoint = e.session_state();
    assert!(h.query_by_label("Document changed; prepare synchronization again.").is_some());
    click(&mut h, "Apply synchronization");
    assert_eq!(e.session_state(), checkpoint, "disabled stale apply does nothing");
    click(&mut h, "Prepare synchronization");
    settle(&mut h, "13 proposed changes");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::agent("other")).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("Apply synchronization").is_none());
}
