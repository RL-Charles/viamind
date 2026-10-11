use serde_json::json;
use viamind_doc::{BoardOutline, Project};
use viamind_engine::EngineHandle;
use viamind_mcp::{CallContext, engine_registry};
#[tokio::test]
async fn independent_agents_route_atomic_polylines_and_reject_stale_session_cursors() {
    let mut project = Project::new("MCP route");
    let p = |x, y| json!({"x":x,"y":y});
    project.board.outline = Some(
        serde_json::from_value::<BoardOutline>(json!({"outer":[p(0,0),p(100_000_000,0),p(100_000_000,100_000_000),p(0,100_000_000)],"cutouts":[]}))
            .unwrap(),
    );
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let a = CallContext::new("route-a");
    let b = CallContext::new("route-b");
    let state = registry.call("get_session_state", json!({}), a.clone()).await.unwrap();
    let input = json!({"expected_project":state["project"]["id"],"expected_cursor":state["cursor"],"points":[p(10_000_000,10_000_000),p(30_000_000,10_000_000),p(30_000_000,20_000_000)],"grid":250000,"width":250000,"layer":"top","net":"GND"});
    let result = registry.call("route_polyline", input.clone(), a.clone()).await.unwrap();
    assert_eq!(result["created"].as_array().unwrap().len(), 2);
    let before = e.session_state();
    assert!(registry.call("route_polyline", input, b).await.unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), before);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert!(e.read(|p| p.board.tracks.is_empty()));
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}
