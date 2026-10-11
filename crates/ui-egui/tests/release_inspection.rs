use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;
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
fn inspect(h: &mut Harness<'_, ViamindApp>) {
    h.get_by_label("Inspect release").scroll_to_me();
    h.run_steps(5);
    let node = h.get_by_label("Inspect release");
    assert!(node.rect().min.x >= 0. && node.rect().max.y <= 400.);
    node.click();
    h.run_steps(3);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().release_inspection.busy() && std::time::Instant::now() < end {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().release_inspection.busy());
}
#[test]
fn compact_historical_inspection_reaches_controls_results_and_error_recovery_with_shared_mcp_parity() {
    let temp = tempfile::tempdir().unwrap();
    let source = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/output-job-population.viamind.json")).unwrap();
    let mut job = viamind_fab::OutputJob::new("Compact reader fixture".into(), 0.);
    job.version = 3;
    job.include_calibrated_placement = Some(true);
    job.include_populated_bom = Some(true);
    let path = temp.path().join("release");
    viamind_fab::OutputJobSnapshot::capture(&source, job)
        .unwrap()
        .write(viamind_fab::ReleaseCursor { stream_id: source.id, sequence: 13 }, &path, None)
        .unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("Unrelated live design"));
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_release_inspection();
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    input(&mut h, "Saved release directory", path.to_str().unwrap());
    inspect(&mut h);
    let result = h.state().release_inspection.result.clone().unwrap();
    assert_eq!(result["matched_assembly"]["component_count"], 2);
    let text = "Matched populated component IDs: 2";
    h.get_by_label(text).scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label(text).rect().max.y <= 400.);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let tools = viamind_mcp::engine_registry(engine.clone()).unwrap().read_only();
    let mcp = runtime.block_on(tools.call("inspect_output_release", serde_json::json!({"path":path}), Default::default())).unwrap();
    assert_eq!(mcp, result);
    input(&mut h, "Saved release directory", temp.path().join("missing").to_str().unwrap());
    assert!(h.state().release_inspection.result.is_none());
    inspect(&mut h);
    assert!(h.state().release_inspection.message.is_some());
    input(&mut h, "Saved release directory", path.to_str().unwrap());
    inspect(&mut h);
    assert!(h.state().release_inspection.result.is_some());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
