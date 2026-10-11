use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::Project;
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;

#[test]
fn compact_read_only_status_keeps_human_menu_placement_and_keyboard_undo_live() {
    let engine = EngineHandle::new(Project::new("Human remains live"));
    let app = ViamindApp::with_design_agent_mode(engine.clone(), Some("http://127.0.0.1:7925/mcp".into()), true);
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Agents read only");
    assert!(h.query_by_label("MCP unavailable").is_none());
    let before = engine.session_state();
    h.get_by_label("Edit").click();
    h.run_steps(4);
    h.get_by_label("Place test resistor").click();
    h.run_steps(5);
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);
    assert!(engine.document_status().dirty);
    assert!(h.state().design.as_ref().unwrap().agent_read_only);
    h.event(Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(4);
    assert_eq!(engine.read(Clone::clone), before.project);
    h.state_mut().set_schematic_mode(true);
    h.run_steps(4);
    h.get_by_label("Agents read only");
}

#[test]
fn unavailable_endpoint_is_distinct_and_default_live_helpers_remain_live() {
    let engine = EngineHandle::new(Project::new("Endpoint status"));
    let app = ViamindApp::with_design_agent_mode(engine.clone(), None, true);
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(4);
    h.get_by_label("Agents read only");
    h.get_by_label("MCP unavailable");
    let live = ViamindApp::with_design(engine, None);
    assert!(!live.design.as_ref().unwrap().agent_read_only);
}
