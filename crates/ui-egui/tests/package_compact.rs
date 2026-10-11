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
