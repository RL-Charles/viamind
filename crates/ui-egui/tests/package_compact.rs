#[path = "support/package_compact.rs"]
mod support;
use support::*;

#[test]
fn actual_compact_complete_power_pin_row_save_and_atomic_package_place() {
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let mut h = harness(&e);
    original(&mut h);
    input(&mut h, "Pin 2 X (mm)", "1.000003");
    input(&mut h, "Pin 2 Y (mm)", "-2.000007");
    click(&mut h, "Pin 2 electrical type");
    click(&mut h, "Input");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("complete.json");
    input(&mut h, "Package file path (load or NEW save)", &path.to_string_lossy());
    click(&mut h, "Save NEW complete package file");
    wait(&mut h, "Created ");
    let saved = viamind_doc::symbol_package::load(&path).unwrap();
    assert_eq!(saved.units.len(), 5);
    assert_eq!(saved.units[4].pins[1].position.x.0, 1_000_003);
    assert_eq!(saved.units[4].pins[1].position.y.0, -2_000_007);
    assert_eq!(saved.units[4].pins[1].electrical, viamind_doc::schematic::PinType::Input);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    click(&mut h, "Place complete package");
    wait(&mut h, "Complete package command applied");
    let placed = e.session_state();
    let c = placed.project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
    assert_eq!(c.packages.len(), 1);
    assert_eq!(c.symbols.len(), 5);
    assert_eq!(c.symbols.values().map(|s| s.pins.len()).sum::<usize>(), 14);
    viamind_ui_egui::actions::UiAction::Undo.run(&e, serde_json::json!({})).unwrap();
    assert_eq!(e.session_state().project, before.project);
}

#[test]
fn actual_compact_author_all_five_units_from_scratch_save_place_and_undo() {
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let mut h = harness(&e);
    authored(&mut h);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("authored.json");
    input(&mut h, "Package file path (load or NEW save)", &path.to_string_lossy());
    click(&mut h, "Save NEW complete package file");
    wait(&mut h, "Created ");
    let saved = viamind_doc::symbol_package::load(&path).unwrap();
    let expected =
        viamind_doc::symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json"))
            .unwrap();
    assert_eq!(saved, expected);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    click(&mut h, "Place complete package");
    wait(&mut h, "Committed package ");
    let report = e.query_symbol_packages(&Default::default()).unwrap();
    assert_eq!(report.total, 1);
    assert_eq!(report.packages[0].units.len(), 5);
    for unit in &report.packages[0].units {
        assert!(h.query_by_label_contains(&unit.symbol_id.to_string()).is_some(), "committed unit identity visible in result");
    }
    viamind_ui_egui::actions::UiAction::Undo.run(&e, serde_json::json!({})).unwrap();
    assert_eq!(e.session_state().project, before.project);
}

#[test]
fn actual_libraries_menu_opens_detached_package_editor() {
    use egui_kittest::{Harness, kittest::Queryable};
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let app = viamind_ui_egui::ViamindApp::with_design(e.clone(), None);
    let mut h =
        Harness::builder().with_size(egui::vec2(640., 400.)).build_ui_state(|ui, app: &mut viamind_ui_egui::ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Libraries").click();
    h.run_steps(5);
    h.get_by_label("Multi-unit package definitions…").click();
    h.run_steps(5);
    assert!(h.query_by_label("Native multi-unit package library").is_some());
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
