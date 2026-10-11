//! The app on a live design: edits from an agent (any thread) show up on the next frame, are
//! attributed to the agent, and the human can undo them with the keyboard or the Edit menu.

use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
use viamind_ui_egui::layers::LayerId;

fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), Some("http://127.0.0.1:0/mcp".into())))
}

fn copper_items(app: &ViamindApp) -> usize {
    app.scene.items.iter().filter(|i| i.layer == LayerId::F_CU).count()
}

#[test]
fn agent_edit_appears_and_user_undoes_it() {
    let engine = EngineHandle::new(Project::new("live"));
    let mut h = harness(&engine);
    h.run_steps(2);
    assert_eq!(copper_items(h.state()), 0);
    h.get_by_label("Edit");

    // An agent places a resistor from another thread, as the MCP server does.
    let agent = engine.clone();
    std::thread::spawn(move || {
        let input = json!({ "designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 30.0, "y_mm": 30.0 });
        agent.execute("add_part", input, Origin::agent("claude-code#1")).unwrap();
    })
    .join()
    .unwrap();

    h.run_steps(1);
    let app = h.state();
    assert_eq!(copper_items(app), 2, "two pads drawn for the resistor");
    let design = app.design.as_ref().unwrap();
    assert_eq!(design.change_count, 1);
    assert!(design.last_change.as_deref().unwrap().contains("agent claude-code#1"));

    // The human presses Ctrl+Z.
    h.event(Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    assert_eq!(engine.read(|p| p.board.parts.len()), 0);
    assert_eq!(copper_items(h.state()), 0);
    assert!(h.state().design.as_ref().unwrap().last_change.as_deref().unwrap().starts_with("undid"));
}

#[test]
fn edit_menu_places_a_resistor_as_the_user() {
    let engine = EngineHandle::new(Project::new("menu"));
    let mut h = harness(&engine);
    h.run_steps(1);
    h.state_mut().place_test_resistor();
    h.state_mut().place_test_resistor();
    h.run_steps(1);
    let designators: Vec<String> = engine.read(|p| p.board.parts.values().map(|part| part.designator.clone()).collect());
    assert_eq!(designators.len(), 2);
    assert!(designators.contains(&"R1".into()) && designators.contains(&"R2".into()));
    assert_eq!(copper_items(h.state()), 4);
}
