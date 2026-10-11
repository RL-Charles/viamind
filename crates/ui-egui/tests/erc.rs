//! Actual input tests over the production desktop ERC window and worker.
use egui::vec2;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, ErcInput, Origin};
use viamind_ui_egui::ViamindApp;

fn engine() -> EngineHandle {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/schematic/regulator.viamind.json");
    EngineHandle::new(viamind_doc::file::load(path).unwrap())
}
fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1200.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, text: &str) {
    h.get_by_label(text).click();
    h.run_steps(5);
}
fn finish(h: &mut Harness<'_, ViamindApp>) {
    for _ in 0..200 {
        h.run_steps(2);
        if h.query_by_label("ERC running; editing remains available.").is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("ERC worker did not finish");
}
#[test]
fn real_erc_run_navigates_exact_pin_without_mutation_and_blocks_stale_results() {
    let e = engine();
    let expected = e.run_erc(&ErcInput::default()).unwrap();
    assert_eq!(expected.violations.len(), 2);
    let pin = expected.violations[0].pins[0].clone();
    let label = format!("Show pin {} · {}", pin.pin_number.as_ref().unwrap(), &pin.id.to_string()[28..]);
    let mut h = harness(&e);
    click(&mut h, "Electrical rule check…");
    let before = e.session_state();
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label_contains("2 errors · 0 warnings").is_some());
    assert_eq!(e.session_state(), before);
    h.state_mut().set_schematic_mode(false);
    h.run_steps(5);
    assert!(!h.state().schematic.active);
    click(&mut h, &label);
    assert!(h.state().schematic.active);
    let at = pin.position.unwrap();
    assert_eq!(h.state().schematic.camera.center, viamind_ui_egui::camera::WorldPos::new(at.x.to_mm(), at.y.to_mm()));
    assert_eq!(e.session_state(), before, "navigation only changes local view state");
    let sheet = e.read(|p| p.schematic.sheets.values().next().unwrap().clone());
    e.execute(
        "edit_schematic_sheet",
        json!({"expected_project":before.project.id,"sheet_id":sheet.id,"expected":sheet,"name":"Peer renamed","edits":[]}),
        Origin::agent("erc-peer"),
    )
    .unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("ERC result is stale: document changed; run ERC again.").is_some());
    assert!(h.get_by_label(&label).accesskit_node().is_disabled());
    let after = e.session_state();
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label("ERC result is stale: document changed; run ERC again.").is_none());
    assert_eq!(e.session_state(), after);
}
#[test]
fn real_policy_form_updates_results_and_same_project_reopen_clears_old_reports() {
    let e = engine();
    let mut h = harness(&e);
    h.state_mut().begin_erc();
    h.run_steps(5);
    let before = e.session_state();
    click(&mut h, "Run ERC");
    finish(&mut h);
    click(&mut h, "ERC rules");
    click(&mut h, "Missing power");
    click(&mut h, "Warning");
    assert!(h.query_by_label("ERC result uses previous rules; run ERC again.").is_some());
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label_contains("0 errors · 2 warnings").is_some());
    assert_eq!(e.session_state(), before);
    click(&mut h, "Reset native ERC rules");
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label_contains("2 errors · 0 warnings").is_some());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), ..Default::default() }).unwrap();
    let saved = e.session_state();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: Some(saved.cursor) }, Origin::User).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label_contains("errors ·").is_none());
    assert!(h.query_by_label_contains("Show pin").is_none());
    assert_eq!(e.read(|p| p.id), before.project.id);
    assert_ne!(e.session_state().cursor.stream_id, before.cursor.stream_id);
}
#[test]
fn empty_project_states_supported_scope_and_ui_catalog_matches_mcp_tool() {
    let e = EngineHandle::new(Project::new("Empty"));
    let mut h = harness(&e);
    h.state_mut().begin_erc();
    h.run_steps(5);
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label("No issues in supported checks. Full ERC is not performed.").is_some());
    e.execute("add_part", json!({"designator":"R1","value":"preview","footprint":"preview","x_mm":0,"y_mm":0}), Origin::agent("editor")).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("No issues in supported checks. Full ERC is not performed.").is_none());
    assert!(h.query_by_label("Previous snapshot: no issues in supported checks; run ERC for current results.").is_some());
    let value = viamind_ui_egui::actions::UiAction::RunErc.run(&e, json!({})).unwrap();
    assert_eq!(value["coverage"]["full_erc"], false);
    assert!(viamind_ui_egui::actions::UiAction::RunErc.run(&e, json!({"max_violations":0})).is_err());
}

#[test]
fn actual_matrix_override_in_narrow_layout_changes_two_output_conflict_severity() {
    use viamind_doc::schematic::{Coordinate, PinType};
    let mut project = engine().read(Clone::clone);
    let sheet = project.schematic.sheets.values_mut().next().unwrap();
    let content = sheet.content.as_mut().unwrap();
    content.wires.clear();
    content.labels.clear();
    content.junctions.clear();
    let mut symbols: Vec<_> = content.symbols.values().take(2).cloned().collect();
    for symbol in &mut symbols {
        symbol.position = Coordinate::default();
        symbol.rotation_quarters = 0;
        symbol.mirrored = false;
        symbol.pins.truncate(1);
        symbol.pins[0].position = Coordinate::default();
        symbol.pins[0].electrical = PinType::Output;
    }
    content.symbols = symbols.into_iter().map(|s| (s.id, s)).collect();
    let e = EngineHandle::new(project);
    let mut h = Harness::builder()
        .with_size(vec2(760.0, 1200.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    h.state_mut().begin_erc();
    h.run_steps(5);
    let before = e.session_state();
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label_contains("1 errors · 0 warnings").is_some());
    click(&mut h, "ERC rules");
    click(&mut h, "Pin compatibility (66 unordered pairs)");
    h.get_by_label("Output / Output").scroll_to_me();
    h.run_steps(5);
    click(&mut h, "Output / Output");
    click(&mut h, "Warning");
    click(&mut h, "Pin compatibility (66 unordered pairs)");
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_by_label_contains("0 errors · 1 warnings").is_some());
    assert_eq!(e.session_state(), before);
}

#[test]
fn virtualized_results_reach_final_diagnostic_after_large_scroll() {
    use viamind_doc::{
        ObjectId,
        schematic::{Coordinate, PinType},
    };
    let mut project = engine().read(Clone::clone);
    let sheet = project.schematic.sheets.values_mut().next().unwrap();
    let content = sheet.content.as_mut().unwrap();
    let seed = content.symbols.values().next().unwrap().clone();
    content.symbols.clear();
    content.wires.clear();
    content.junctions.clear();
    content.labels.clear();
    for i in 0..20 {
        let mut symbol = seed.clone();
        symbol.id = ObjectId::new();
        symbol.designator = format!("U{i}");
        symbol.position = Coordinate { x: viamind_geom::Nm(i * 1_000_000), y: viamind_geom::Nm(0) };
        symbol.pins.truncate(1);
        symbol.pins[0].id = ObjectId::new();
        symbol.pins[0].position = Coordinate::default();
        symbol.pins[0].electrical = PinType::Input;
        content.symbols.insert(symbol.id, symbol);
    }
    let e = EngineHandle::new(project);
    let report = e.run_erc(&ErcInput::default()).unwrap();
    assert_eq!(report.violations.len(), 40);
    let pin = &report.violations.last().unwrap().pins[0];
    let caption = format!("Show pin {} · {}", pin.pin_number.as_ref().unwrap(), &pin.id.to_string()[28..]);
    let mut h = harness(&e);
    h.state_mut().begin_erc();
    h.run_steps(5);
    click(&mut h, "Run ERC");
    finish(&mut h);
    assert!(h.query_all_by_label(&caption).next().is_none());
    h.event(egui::Event::PointerMoved(egui::pos2(300.0, 300.0)));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -100000.0),
        phase: egui::TouchPhase::Move,
        modifiers: Default::default(),
    });
    h.run_steps(30);
    assert!(h.query_all_by_label(&caption).next().is_some(), "Final diagnostic must remain reachable at the end of the virtual list");
}

#[test]
fn first_erc_navigation_from_pcb_preserves_exact_pin_center_through_initial_schematic_layout() {
    let e = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
    let expected = e.run_erc(&ErcInput::default()).unwrap();
    let pin = expected.violations[0].pins[0].clone();
    let mut h = harness(&e);
    assert!(!h.state().schematic.active);
    h.state_mut().begin_erc();
    h.run_steps(5);
    click(&mut h, "Run ERC");
    finish(&mut h);
    let before = e.session_state();
    let label = format!("Show pin {} · {}", pin.pin_number.as_ref().unwrap(), &pin.id.to_string()[28..]);
    click(&mut h, &label);
    assert!(h.state().schematic.active);
    let at = pin.position.unwrap();
    assert_eq!(h.state().schematic.camera.center, viamind_ui_egui::camera::WorldPos::new(at.x.to_mm(), at.y.to_mm()));
    assert_eq!(e.session_state(), before);
}
