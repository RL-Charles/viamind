use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{CallContext, engine_registry};
#[tokio::test]
async fn independent_agent_copper_edits_share_ids_and_own_undo() {
    let engine = EngineHandle::new(Project::new("MCP Copper"));
    let registry = engine_registry(engine.clone()).unwrap();
    let a = CallContext::new("route-a");
    let b = CallContext::new("route-b");
    let project = engine.read(|p| p.id);
    let track = registry
        .call(
            "add_track",
            json!({"expected_project":project,"start_x_mm":1,"start_y_mm":2,"end_x_mm":5,"end_y_mm":6,"width_mm":0.25,"layer":"top","net":"VCC"}),
            a.clone(),
        )
        .await
        .unwrap();
    let via = registry
        .call("add_via", json!({"expected_project":project,"x_mm":5,"y_mm":6,"diameter_mm":0.6,"drill_mm":0.3,"net":"VCC"}), b.clone())
        .await
        .unwrap();
    let design = registry.call("get_design", json!({}), a.clone()).await.unwrap();
    assert_eq!(design["board"]["tracks"][track["created"][0].as_str().unwrap()]["width"], 250000);
    assert_eq!(design["board"]["vias"][via["created"][0].as_str().unwrap()]["drill"], 300000);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert!(engine.read(|p| p.board.tracks.is_empty() && p.board.vias.len() == 1));
    registry.call("redo", json!({}), a.clone()).await.unwrap();
    registry.call("delete_track", json!({"id":track["created"][0]}), a).await.unwrap();
    registry.call("delete_via", json!({"id":via["created"][0]}), b).await.unwrap();
    engine.undo(Origin::User).unwrap();
    assert!(engine.read(|p| p.board.vias.len() == 1));
}
