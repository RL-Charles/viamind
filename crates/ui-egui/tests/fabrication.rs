//! Actual accessible form verifies the asynchronous immutable export surface.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(text);
    h.run_steps(5);
}
#[test]
fn export_window_keeps_invalid_input_then_exports_snapshot_without_design_changes() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_fabrication_export();
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fabrication");
    input(&mut h, "New fabrication directory", path.to_str().unwrap());
    input(&mut h, "Mask expansion (mm)", "-1");
    h.get_by_label("Export fabrication snapshot").click();
    h.run_steps(5);
    assert!(!path.exists());
    assert!(h.query_by_label("Mask expansion needs a finite number from 0 to 10 mm").is_some());
    input(&mut h, "Mask expansion (mm)", "0.05");
    h.get_by_label("Export fabrication snapshot").click();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.run_steps(5);
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 7);
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["mask_expansion_nm"], 50000);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
