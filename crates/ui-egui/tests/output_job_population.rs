//! Actual compact fabrication controls and immutable shared writer parity.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;
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
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().output_jobs.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().output_jobs.busy());
}
#[test]
fn compact_matched_population_dependencies_save_load_run_and_live_mcp_bytes() {
    let _guard = TEST.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/calibration.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_output_jobs();
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Run saved job to NEW directory");
    assert!(h.query_by_label("Enter a saved job file path.").is_some());
    let job = temp.path().join("job.json");
    let release = temp.path().join("release");
    input(&mut h, "Job name", "UI release λ");
    assert!(h.get_by_label("Include matched populated BOM (version 3)").accesskit_node().is_disabled());
    click(&mut h, "Include calibrated placement companion (version 2)");
    click(&mut h, "Include matched populated BOM (version 3)");
    input(&mut h, "Mask expansion (mm)", "-1");
    input(&mut h, "Saved job file", job.to_str().unwrap());
    click(&mut h, "Save NEW job file");
    assert!(!job.exists());
    input(&mut h, "Mask expansion (mm)", "0.050001");
    click(&mut h, "Save NEW job file");
    wait(&mut h);
    assert!(job.exists());
    let saved = viamind_fab::load_output_job(&job).unwrap();
    assert_eq!(saved.version, 3);
    assert_eq!(saved.include_populated_bom, Some(true));
    assert_eq!(saved.include_calibrated_placement, Some(true));
    click(&mut h, "Include calibrated placement companion (version 2)");
    assert!(h.get_by_label("Include matched populated BOM (version 3)").accesskit_node().is_disabled());
    input(&mut h, "Job name", "Unsaved name");
    click(&mut h, "Load job");
    wait(&mut h);
    assert_eq!(viamind_fab::load_output_job(&job).unwrap().name, "UI release λ");
    input(&mut h, "NEW release directory", release.to_str().unwrap());
    h.get_by_label("Run saved job to NEW directory").scroll_to_me();
    h.run_steps(5);
    let rect = h.get_by_label("Run saved job to NEW directory").rect();
    assert!(rect.max.y <= 400. && rect.max.x <= 640., "{rect:?}");
    click(&mut h, "Run saved job to NEW directory");
    wait(&mut h);
    assert!(release.join("release-manifest.json").exists());
    let label = format!("13 files · snapshot {} / {}", before.cursor.stream_id, before.cursor.sequence);
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label(&label).rect().max.y <= 400.);
    let companion = "Matched populated BOM/placement: 2 components; older inventories still include all components.";
    h.get_by_label(companion).scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label(companion).rect().max.y <= 400.);
    assert_eq!(
        std::fs::read_to_string(release.join("assembly/calibrated-placement.csv")).unwrap(),
        viamind_fab::csv(&before.project, viamind_fab::AssemblyCsv::AssemblyPlacement).unwrap()
    );
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(release.join("release-manifest.json")).unwrap()).unwrap();
    let digest = manifest["populated_assembly"]["component_ids_sha256"].as_str().unwrap();
    let digest_label = format!("Included component identity SHA256: {digest}");
    h.get_by_label(&digest_label).scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label(&digest_label).rect().max.y <= 400.);
    let tools = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let mcp = temp.path().join("mcp");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(tools.call("run_output_job", serde_json::json!({"job_path":job,"path":mcp,"expected_cursor":before.cursor}), Default::default()))
        .unwrap();
    for folder in ["assembly", "fabrication", ""] {
        for entry in std::fs::read_dir(release.join(folder)).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(mcp.join(folder).join(path.file_name().unwrap())).unwrap());
            }
        }
    }
    click(&mut h, "Run saved job to NEW directory");
    wait(&mut h);
    assert!(h.query_all_by_label_contains("already exists").next().is_some());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    engine
        .execute("add_via", serde_json::json!({"x_mm":1,"y_mm":1,"diameter_mm":0.6,"drill_mm":0.3}), viamind_engine::Origin::agent("newer"))
        .unwrap();
    // A successful result remains visible after a later edit (repeat into a fresh destination first).
    engine.undo(viamind_engine::Origin::User).unwrap();
    let newer = temp.path().join("before-newer");
    input(&mut h, "NEW release directory", newer.to_str().unwrap());
    click(&mut h, "Run saved job to NEW directory");
    wait(&mut h);
    engine
        .execute("add_via", serde_json::json!({"x_mm":1,"y_mm":1,"diameter_mm":0.6,"drill_mm":0.3}), viamind_engine::Origin::agent("newer"))
        .unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("The design has changed since this exported snapshot.").is_some());
}
