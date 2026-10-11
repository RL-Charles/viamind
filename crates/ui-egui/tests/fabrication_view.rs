use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap())
}
fn source(id: viamind_doc::ObjectId) -> String {
    format!(
        "%FSLAX56Y56*%%MOMM*%%TF.FileFunction,Copper,L1,Top*%%TF.FilePolarity,Positive*%%TF.SameCoordinates,{id}*%%ADD10C,0.600001*%%LPD*%%LMN*%%LS1*%G01*D10*X18000000Y-12000000D03*M02*"
    )
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).focus();
    h.run_steps(3);
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
}
fn wait(h: &mut Harness<'_, ViamindApp>, label: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.query_by_label_contains(label).is_none() {
        assert!(std::time::Instant::now() < end, "missing {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
}
#[test]
fn actual_file_menu_inspection_visibility_and_peer_edit_keep_overlay_current_and_read_only() {
    let _guard = SERIAL.lock().unwrap();
    let engine = engine();
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("top.gbr");
    std::fs::write(&path, source(before.project.id)).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    click(&mut h, "File");
    click(&mut h, "Inspect fabrication files…");
    input(&mut h, "Fabrication file 1 path", path.to_str().unwrap());
    click(&mut h, "Inspect selected fabrication files");
    wait(&mut h, "Inspected 1 files / 1 primitives");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(h.state().fabrication_view.painted_primitives(), 0);
    click(&mut h, "Show fabrication overlay");
    assert_eq!(h.state().fabrication_view.painted_primitives(), 1);
    click(&mut h, "Show fabrication file 1 (TopCopper)");
    assert_eq!(h.state().fabrication_view.painted_primitives(), 0);
    click(&mut h, "Show fabrication file 1 (TopCopper)");
    assert_eq!(h.state().fabrication_view.painted_primitives(), 1);
    engine
        .execute(
            "add_track",
            serde_json::json!({"start_x_mm":30,"start_y_mm":20,"end_x_mm":35,"end_y_mm":20,"width_mm":0.25,"layer":"top","net":"PEER"}),
            Origin::agent("peer"),
        )
        .unwrap();
    h.run_steps(5);
    assert_eq!(h.state().fabrication_view.painted_primitives(), 0);
    assert!(h.query_by_label_contains("Document changed").is_some());
    assert_eq!(std::fs::read_to_string(path).unwrap(), source(before.project.id));
}
#[test]
fn actual_compact_inspect_and_unsupported_error_are_reachable_and_preserve_fields() {
    let _guard = SERIAL.lock().unwrap();
    let engine = engine();
    let before = engine.session_state();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsupported.gbr");
    std::fs::write(&path, "G36*M02*").unwrap();
    let good = dir.path().join("top.gbr");
    std::fs::write(&good, source(before.project.id)).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_fabrication_inspection();
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    input(&mut h, "Fabrication file 1 path", good.to_str().unwrap());
    h.get_by_label("Inspect selected fabrication files").scroll_to_me();
    h.run_steps(5);
    let rect = h.get_by_label("Inspect selected fabrication files").rect();
    assert!(rect.min.x >= 0.0 && rect.max.x <= 640.0 && rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    click(&mut h, "Inspect selected fabrication files");
    wait(&mut h, "Inspected 1 files / 1 primitives");
    h.get_by_label("Show fabrication overlay").scroll_to_me();
    h.run_steps(5);
    let rect = h.get_by_label("Show fabrication overlay").rect();
    assert!(rect.min.x >= 0.0 && rect.max.x <= 640.0 && rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    click(&mut h, "Show fabrication overlay");
    assert_eq!(h.state().fabrication_view.painted_primitives(), 1);
    input(&mut h, "Fabrication file 1 path", path.to_str().unwrap());
    click(&mut h, "Inspect selected fabrication files");
    wait(&mut h, "Inspection failed");
    h.get_by_label_contains("Inspection failed").scroll_to_me();
    h.run_steps(3);
    let rect = h.get_by_label_contains("Inspection failed").rect();
    assert!(rect.min.x >= 0.0 && rect.max.x <= 640.0 && rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    assert!(!h.state().fabrication_view.busy());
    assert_eq!(engine.session_state(), before);
    assert_eq!(h.state().fabrication_view.painted_primitives(), 0);
    assert!(h.query_by_label("Show fabrication overlay").is_none());
}
