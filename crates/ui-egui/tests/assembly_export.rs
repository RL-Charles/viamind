//! Actual accessible CSV controls, shared MCP parity and immutable error paths.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::{Value, json};
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, Origin};
use viamind_fab::{AssemblyCsv, AssemblySnapshot, FabricationSnapshot};
use viamind_ui_egui::{ViamindApp, actions::UiAction};
static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn project() -> Project {
    file::from_str(include_str!("../../../docs/verification/t0075-export.viamind.json")).unwrap()
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
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().assembly_export.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().assembly_export.busy());
}
fn parsed(path: &std::path::Path) -> Value {
    let output = std::process::Command::new("python3")
        .args(["-c", "import csv,json,sys; print(json.dumps(list(csv.DictReader(open(sys.argv[1],newline='')))))"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn file_menu_exports_both_kinds_matches_live_mcp_and_only_overwrites_explicitly() {
    let _guard = TEST.lock().unwrap();
    let engine = EngineHandle::new(project());
    let before = engine.session_state();
    let status = engine.document_status();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "File");
    click(&mut h, "Export assembly CSV…");
    click(&mut h, "Export CSV to NEW file");
    assert!(h.query_by_label("Enter a CSV output file path in an existing directory.").is_some());
    let dir = tempfile::tempdir().unwrap();
    input(&mut h, "CSV output file", dir.path().join("wrong.svg").to_str().unwrap());
    click(&mut h, "Export CSV to NEW file");
    assert!(h.query_by_label("Output path must end in .csv.").is_some());
    for (label, kind) in [("BOM", AssemblyCsv::Bom), ("Placement", AssemblyCsv::Placement)] {
        click(&mut h, label);
        let path = dir.path().join(format!("{label}.csv"));
        input(&mut h, "CSV output file", path.to_str().unwrap());
        click(&mut h, "Export CSV to NEW file");
        wait(&mut h);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes, engine.read(|p| viamind_fab::csv(p, kind)).unwrap().as_bytes());
        let mcp = dir.path().join(format!("mcp-{label}.csv"));
        let result = runtime
            .block_on(registry.call("export_assembly_csv", json!({"kind":kind,"path":mcp,"expected_cursor":before.cursor}), Default::default()))
            .unwrap();
        assert_eq!(result["exported_cursor"], json!(before.cursor));
        assert_eq!(std::fs::read(mcp).unwrap(), bytes);
        let rows = parsed(&path);
        assert!(rows.as_array().unwrap().iter().all(|r| r["Value"] == "10k, \"precision\"\r\nµΩ"));
        if kind == AssemblyCsv::Bom {
            assert_eq!(rows[0]["Quantity"], "2");
        } else {
            assert_eq!(rows[0]["X (mm)"], "30.000000");
            assert_eq!(rows[0]["Side"], "Bottom");
        }
        click(&mut h, "Export CSV to NEW file");
        wait(&mut h);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(h.query_all_by_label_contains("Export failed:").next().is_some());
        std::fs::write(&path, "replace only with explicit permission").unwrap();
        click(&mut h, "Allow replacing this CSV output");
        click(&mut h, "Export CSV (replace allowed)");
        wait(&mut h);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        click(&mut h, "Allow replacing this CSV output");
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    engine
        .execute("add_part", json!({"designator":"R_NEWER","value":"","footprint":"0603","x_mm":0,"y_mm":0}), Origin::agent("other-agent"))
        .unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("The design has changed since this exported snapshot.").is_some());
}
#[test]
fn catalog_stale_busy_budget_and_native_guards_preserve_files_and_state() {
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native.csv");
    file::save(&project(), &native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let original = std::fs::read(&native).unwrap();
    let output = dir.path().join("rejected.csv");
    let a = engine.read(AssemblySnapshot::capture).unwrap();
    let b = engine.read(FabricationSnapshot::capture).unwrap();
    let mut stale = before.cursor;
    stale.sequence += 1;
    let error = UiAction::ExportAssemblyCsv.run(&engine, json!({"kind":"bom","path":output,"expected_cursor":stale})).unwrap_err();
    assert!(error.to_string().contains("document changed"));
    let error = UiAction::ExportAssemblyCsv.run(&engine, json!({"kind":"bom","path":output})).unwrap_err();
    assert!(error.to_string().contains("busy"));
    drop((a, b));
    assert!(UiAction::ExportAssemblyCsv.run(&engine, json!({"kind":"bom","path":native,"overwrite":true})).is_err());
    for bad in [json!({"kind":"bogus","path":output}), json!({"kind":"bom","path":""}), json!({"kind":"bom","path":output,"extra":1})] {
        assert!(UiAction::ExportAssemblyCsv.run(&engine, bad).is_err());
    }
    assert!(!output.exists());
    assert_eq!(std::fs::read(&native).unwrap(), original);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let part = before.project.board.parts.values().next().unwrap();
    engine.execute("update_part", json!({"id":part.id,"value":"x".repeat(4097),"expected":part}), Origin::User).unwrap();
    let edited = engine.session_state();
    std::fs::write(&output, "keep").unwrap();
    assert!(UiAction::ExportAssemblyCsv.run(&engine, json!({"kind":"bom","path":output,"overwrite":true})).is_err());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "keep");
    assert_eq!(engine.session_state(), edited);
}
#[test]
fn compact_form_exports_with_pointer_input_and_native_overwrite_error_stays_visible() {
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native.csv");
    file::save(&project(), &native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let original = std::fs::read(&native).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_assembly_export();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "Placement");
    let path = dir.path().join("compact.csv");
    input(&mut h, "CSV output file", path.to_str().unwrap());
    h.get_by_label("Export CSV to NEW file").scroll_to_me();
    h.run_steps(5);
    let rect = h.get_by_label("Export CSV to NEW file").rect();
    assert!(rect.min.y >= 0.0 && rect.max.y <= 400.0 && rect.max.x <= 640.0, "{rect:?}");
    click(&mut h, "Export CSV to NEW file");
    wait(&mut h);
    assert!(path.is_file());
    let label = format!("Exported {}", path.display());
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label(&label).rect().max.y <= 400.0);
    input(&mut h, "CSV output file", native.to_str().unwrap());
    click(&mut h, "Allow replacing this CSV output");
    click(&mut h, "Export CSV (replace allowed)");
    wait(&mut h);
    let error = h.query_all_by_label_contains("Export failed:").next().unwrap();
    error.scroll_to_me();
    h.run_steps(5);
    assert!(h.query_all_by_label_contains("active native").next().is_some());
    assert_eq!(std::fs::read(native).unwrap(), original);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
