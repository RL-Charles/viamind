//! CI gate: every shared-state UI action must have a matching live MCP tool and schema.
use serde_json::{Value, json};
use std::{fs, path::Path};
use viamind_doc::Project;
use viamind_engine::EngineHandle;
use viamind_mcp::{ToolRegistry, agent::CallContext, engine_registry};
use viamind_ui_egui::{ViamindApp, actions::UiAction};

fn missing_actions(registry: &ToolRegistry) -> Vec<UiAction> {
    UiAction::ALL.iter().copied().filter(|action| registry.get(action.tool()).is_none()).collect()
}

#[test]
fn every_ui_action_has_a_matching_mcp_tool_and_input_schema() {
    let engine = EngineHandle::new(Project::new("parity"));
    let registry = engine_registry(engine.clone()).unwrap();
    assert!(missing_actions(&registry).is_empty(), "UI actions missing MCP tools: {:?}", missing_actions(&registry));
    for action in UiAction::ALL {
        assert_eq!(action.input_schema(), registry.get(action.tool()).unwrap().input_schema, "schema mismatch for {action:?}");
    }
    for spec in engine.specs() {
        let tool = registry.get(&spec.name).unwrap_or_else(|| panic!("engine command {} missing MCP tool", spec.name));
        assert_eq!(spec.input_schema, tool.input_schema);
    }
}

#[test]
fn parity_gate_detects_a_missing_tool() {
    let live = engine_registry(EngineHandle::new(Project::new("negative"))).unwrap();
    let mut broken = ToolRegistry::new();
    for tool in live.tools().filter(|tool| tool.name != "add_part") {
        broken.register(tool.name.clone(), tool.description.clone(), tool.input_schema.clone(), |_| async { Ok(Value::Null) }).unwrap();
    }
    assert_eq!(missing_actions(&broken), [UiAction::PlacePart], "removing a UI tool must trip the gate");
}

fn check_source(path: &Path) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check_source(&path);
        } else if path.extension().is_some_and(|ext| ext == "rs") && path.file_name().unwrap() != "actions.rs" {
            let source: String = fs::read_to_string(&path).unwrap().chars().filter(|c| !c.is_whitespace()).collect();
            for method in [
                "execute",
                "execute_typed",
                "execute_batch",
                "transaction",
                "undo",
                "redo",
                "undo_own",
                "redo_own",
                "set_selection",
                "save_project",
                "new_project",
                "open_project",
                "import_footprint",
            ] {
                assert!(
                    !source.contains(&format!(".{method}(")) && !source.contains(&format!(".{method}::<")),
                    "{} bypasses the UI action catalog with {method}; add an action in actions.rs so MCP parity can check it",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn ui_mutations_route_through_the_catalog() {
    check_source(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}

#[tokio::test]
async fn ui_and_agent_actions_operate_on_the_same_engine() {
    let engine = EngineHandle::new(Project::new("shared"));
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.place_test_resistor();
    let user_part = engine.read(|p| p.board.part_by_designator("R1").unwrap().id);
    let registry = engine_registry(engine.clone()).unwrap();
    let agent = CallContext::new("parity-agent");
    let result = registry
        .call("add_part", json!({ "designator": "R2", "value": "1k", "footprint": "R_0603", "x_mm": 30, "y_mm": 20 }), agent.clone())
        .await
        .unwrap();
    let id = result["created"][0].clone();
    UiAction::SetSelection.run(&engine, json!({ "ids": [user_part] })).unwrap();
    let selection = registry.call("get_selection", json!({}), agent.clone()).await.unwrap();
    assert_eq!(selection["ids"], json!([user_part]));
    registry.call("set_selection", json!({ "ids": [id] }), agent.clone()).await.unwrap();
    assert_eq!(serde_json::to_value(engine.selection().ids).unwrap(), json!([id]));
    // The UI undoes any origin; the agent undoes only its own edits.
    app.undo_design();
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);
    assert!(engine.selection().ids.is_empty());
    app.redo_design();
    registry.call("undo", json!({}), agent.clone()).await.unwrap();
    assert!(engine.read(|p| p.board.parts.contains_key(&user_part)));
    registry.call("redo", json!({}), agent).await.unwrap();
    assert_eq!(engine.read(|p| p.board.parts.len()), 2);
}
