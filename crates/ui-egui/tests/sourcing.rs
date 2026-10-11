//! Actual compact input and shared engine/MCP export parity.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::{ViamindApp, actions::UiAction};
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(5);
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
#[test]
fn compact_sourcing_is_guarded_undoable_and_matches_live_mcp_csv() {
    let engine = EngineHandle::new(Project::new("Compact sourcing"));
    let id =
        engine.execute("add_part", json!({"designator":"R1","value":"10k","footprint":"R_0603","x_mm":1,"y_mm":2}), Origin::User).unwrap().created[0];
    UiAction::SetSelection.run(&engine, json!({"ids":[id]})).unwrap();
    let before = engine.read(Clone::clone);
    let old_bom = engine.read(|p| viamind_fab::csv(p, viamind_fab::AssemblyCsv::Bom)).unwrap();
    let mut h = Harness::builder()
        .with_size(vec2(640.0, 400.0))
        .build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    input(&mut h, "Manufacturer", "µ, \"User input\"");
    input(&mut h, "Manufacturer part number", "USER-MPN");
    input(&mut h, "Supplier", "Example supplier");
    input(&mut h, "Supplier stock number", "USER-SKU");
    click(&mut h, "Populate component");
    click(&mut h, "Apply sourcing");
    let after = engine.read(Clone::clone);
    let m = after.board.part(&id).unwrap().assembly.as_ref().unwrap();
    assert_eq!(m.manufacturer.as_deref(), Some("µ, \"User input\""));
    assert!(!m.populate);
    assert_eq!(engine.read(|p| viamind_fab::csv(p, viamind_fab::AssemblyCsv::Bom)).unwrap(), old_bom);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    h.run_steps(5);
    input(&mut h, "Manufacturer part number", "STALE-DRAFT");
    engine.execute("update_part", json!({"id":id,"value":"peer edit"}), Origin::agent("peer")).unwrap();
    h.run_steps(5);
    click(&mut h, "Apply sourcing");
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().assembly.as_ref().unwrap().mpn.clone()).as_deref(), Some("USER-MPN"));
    click(&mut h, "Reload");
    input(&mut h, "Manufacturer part number", "invalid trailing ");
    let state = engine.session_state();
    click(&mut h, "Apply sourcing");
    assert_eq!(engine.session_state(), state);
    click(&mut h, "Reload");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mcp = directory.path().join("mcp.csv");
    let desktop = directory.path().join("desktop.csv");
    runtime
        .block_on(registry.call(
            "export_assembly_csv",
            json!({"kind":"sourcing_bom","path":mcp,"expected_cursor":engine.event_cursor()}),
            Default::default(),
        ))
        .unwrap();
    UiAction::ExportAssemblyCsv.run(&engine, json!({"kind":"sourcing_bom","path":desktop,"expected_cursor":engine.event_cursor()})).unwrap();
    assert_eq!(std::fs::read(mcp).unwrap(), std::fs::read(desktop).unwrap());
    assert_eq!(engine.session_state(), state);
    click(&mut h, "Clear sourcing");
    assert!(engine.read(|p| p.board.part(&id).unwrap().assembly.is_none()));
}
