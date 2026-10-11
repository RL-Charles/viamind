//! Actual compact property editing and shared guarded CSV publication.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
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
fn compact_calibration_input_history_preservation_and_export_match_mcp() {
    let engine = EngineHandle::new(viamind_doc::Project::new("Assembly calibration"));
    let id = engine
        .execute(
            "add_part",
            json!({"designator":"U1","value":"µ, part","footprint":"asymmetric","x_mm":10,"y_mm":20,"rotation_deg":90,"side":"bottom"}),
            Origin::User,
        )
        .unwrap()
        .created[0];
    UiAction::SetSelection.run(&engine, json!({"ids":[id]})).unwrap();
    let original = engine.read(Clone::clone);
    let mut h = Harness::builder()
        .with_size(vec2(640., 400.))
        .build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    click(&mut h, "Surface mount");
    click(&mut h, "Explicit placement calibration");
    input(&mut h, "Local centroid X (mm)", "2");
    input(&mut h, "Local centroid Y (mm)", "1");
    input(&mut h, "Local zero-axis marker (deg)", "37.125");
    h.get_by_label("Apply assembly metadata").scroll_to_me();
    h.run_steps(5);
    assert!(h.get_by_label("Apply assembly metadata").rect().max.y <= 400.);
    click(&mut h, "Apply assembly metadata");
    let calibrated = engine.read(Clone::clone);
    let metadata = calibrated.board.parts[&id].assembly.as_ref().unwrap();
    assert_eq!(metadata.mounting, Some(viamind_doc::MountingTechnology::SurfaceMount));
    assert_eq!(metadata.placement.unwrap().centroid_local, viamind_geom::Point::from_mm(2., 1.));
    assert_eq!(metadata.placement.unwrap().zero_axis_mdeg, 37_125);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), original);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), calibrated);
    h.run_steps(5);
    input(&mut h, "Manufacturer part number", "EXPLICIT");
    click(&mut h, "Apply assembly metadata");
    assert_eq!(engine.read(|p| p.board.parts[&id].assembly.as_ref().unwrap().placement), metadata.placement);
    h.run_steps(5);
    let unchanged = engine.session_state();
    for value in ["359.9996", "-0.0001", "NaN"] {
        input(&mut h, "Local zero-axis marker (deg)", value);
        click(&mut h, "Apply assembly metadata");
        assert_eq!(engine.session_state(), unchanged);
    }
    input(&mut h, "Local zero-axis marker (deg)", "37.125");
    input(&mut h, "Local centroid X (mm)", "10000.0000004");
    click(&mut h, "Apply assembly metadata");
    assert_eq!(engine.session_state(), unchanged);
    click(&mut h, "Reload");
    let unchanged = engine.session_state();
    let directory = tempfile::tempdir().unwrap();
    let desktop = directory.path().join("desktop.csv");
    let mcp = directory.path().join("agent.csv");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let result = runtime
        .block_on(registry.call(
            "export_assembly_csv",
            json!({"kind":"assembly_placement","path":mcp,"expected_cursor":engine.event_cursor()}),
            Default::default(),
        ))
        .unwrap();
    assert_eq!(result["component_count"], 1);
    click(&mut h, "File");
    click(&mut h, "Export assembly CSV…");
    click(&mut h, "Calibrated placement");
    input(&mut h, "CSV output file", desktop.to_str().unwrap());
    click(&mut h, "Export CSV to NEW file");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().assembly_export.busy() && std::time::Instant::now() < deadline {
        h.run_steps(3);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().assembly_export.busy());
    assert_eq!(std::fs::read(&desktop).unwrap(), std::fs::read(mcp).unwrap());
    assert!(std::fs::read_to_string(desktop).unwrap().contains("11.000000,-22.000000,232.875,bottom"));
    assert_eq!(engine.session_state(), unchanged);
}
