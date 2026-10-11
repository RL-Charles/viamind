//! Actual compact fabrication controls and immutable shared writer parity.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::EngineHandle;
use viamind_ui_egui::{ViamindApp, actions::UiAction};
static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
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
    let _guard = TEST.lock().unwrap();
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
    h.run_steps(3);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.state().fabrication.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.run_steps(5);
    assert!(!h.state().fabrication.busy());
    assert!(h.query_by_label(&format!("Snapshot {} / {}", before.cursor.stream_id, before.cursor.sequence)).is_some());
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 7);
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["mask_expansion_nm"], 50000);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}

fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().fabrication.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().fabrication.busy());
}
fn files(path: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_str().unwrap().into(), std::fs::read(p).unwrap())
        })
        .collect()
}
#[test]
fn compact_form_full_cursor_live_mcp_bytes_and_collision_native_errors_are_accessible() {
    use serde_json::json;
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native.json");
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    viamind_doc::file::save(&project, &native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let original = std::fs::read(&native).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_fabrication_export();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "Export fabrication snapshot");
    assert!(h.query_by_label("Enter a NEW fabrication directory in an existing parent directory.").is_some());
    let output = dir.path().join("compact-fab");
    input(&mut h, "New fabrication directory", output.to_str().unwrap());
    input(&mut h, "Mask expansion (mm)", "NaN");
    click(&mut h, "Export fabrication snapshot");
    assert!(!output.exists());
    input(&mut h, "Mask expansion (mm)", "0.050001");
    h.get_by_label("Export fabrication snapshot").scroll_to_me();
    h.run_steps(5);
    let rect = h.get_by_label("Export fabrication snapshot").rect();
    assert!(rect.min.y >= 0.0 && rect.max.y <= 400.0 && rect.max.x <= 640.0, "{rect:?}");
    click(&mut h, "Export fabrication snapshot");
    wait(&mut h);
    let label = format!("Snapshot {} / {}", before.cursor.stream_id, before.cursor.sequence);
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label(&label).rect().max.y <= 400.0);
    let actual = files(&output);
    let expected =
        viamind_fab::fabrication(&before.project, viamind_fab::FabricationOptions { mask_expansion_mm: 0.050001, include_silkscreen: false })
            .unwrap();
    assert_eq!(actual, expected.files.into_iter().map(|(k, v)| (k, v.into_bytes())).collect());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let mcp = dir.path().join("mcp-fab");
    let result = runtime
        .block_on(registry.call(
            "export_fabrication",
            json!({"path":mcp,"mask_expansion_mm":0.050001,"expected_cursor":before.cursor}),
            Default::default(),
        ))
        .unwrap();
    assert_eq!(result["exported_cursor"], json!(before.cursor));
    assert_eq!(files(&mcp), actual);
    click(&mut h, "Export fabrication snapshot");
    wait(&mut h);
    assert_eq!(files(&output), actual);
    let error = h.query_all_by_label_contains("Export failed:").next().unwrap();
    error.scroll_to_me();
    h.run_steps(5);
    assert!(h.query_all_by_label_contains("Export failed:").next().unwrap().rect().max.y <= 400.0);
    input(&mut h, "New fabrication directory", native.to_str().unwrap());
    click(&mut h, "Export fabrication snapshot");
    wait(&mut h);
    assert!(h.query_all_by_label_contains("active native project").next().is_some());
    assert_eq!(std::fs::read(&native).unwrap(), original);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let after = dir.path().join("before-newer-edit");
    input(&mut h, "New fabrication directory", after.to_str().unwrap());
    click(&mut h, "Export fabrication snapshot");
    wait(&mut h);
    engine
        .execute(
            "add_part",
            json!({"designator":"R_NEWER","value":"","footprint":"0603","x_mm":0,"y_mm":0}),
            viamind_engine::Origin::agent("newer-editor"),
        )
        .unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("The design has changed since this exported snapshot.").is_some());
}
#[test]
fn typed_action_validates_after_cursor_before_admission_and_preserves_state_on_failures() {
    use serde_json::json;
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new-fab");
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let a = engine.read(viamind_fab::AssemblySnapshot::capture).unwrap();
    let b = engine.read(viamind_fab::FabricationSnapshot::capture).unwrap();
    let mut stale = before.cursor;
    stale.sequence += 1;
    let error = UiAction::ExportFabrication.run(&engine, json!({"path":path,"mask_expansion_mm":-1,"expected_cursor":stale})).unwrap_err();
    assert!(error.to_string().contains("document changed"));
    let error = UiAction::ExportFabrication.run(&engine, json!({"path":path})).unwrap_err();
    assert!(error.to_string().contains("busy"));
    for input in [
        json!({"path":""}),
        json!({"path":path,"mask_expansion_mm":-1}),
        json!({"path":path,"mask_expansion_mm":11}),
        json!({"path":path,"extra":true}),
    ] {
        assert!(UiAction::ExportFabrication.run(&engine, input).is_err());
    }
    drop((a, b));
    assert!(!path.exists());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
