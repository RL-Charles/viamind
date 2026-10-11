use serde_json::json;
use viamind_doc::{ObjectId, Project, Track, Via};
use viamind_engine::EngineHandle;
use viamind_mcp::{CallContext, engine_registry};
#[tokio::test]
async fn two_clients_discover_existing_copper_and_commit_atomic_shared_commands_read_only_parity() {
    let mut project = Project::new("MCP terminal route");
    project.board.outline = Some(
        serde_json::from_value(json!({"outer":[{"x":0,"y":0},{"x":50000000,"y":0},{"x":50000000,"y":50000000},{"x":0,"y":50000000}],"cutouts":[]}))
            .unwrap(),
    );
    let t: Track = serde_json::from_value(
        json!({"id":ObjectId::new(),"start":{"x":5000000,"y":10000000},"end":{"x":10000003,"y":10000007},"width":500000,"layer":"top","net":"N"}),
    )
    .unwrap();
    let v: Via =
        serde_json::from_value(json!({"id":ObjectId::new(),"position":{"x":30000001,"y":25000003},"diameter":1000000,"drill":300000,"net":"N"}))
            .unwrap();
    let input_terminal =
        |kind, id| if kind == "track_end" { json!({"kind":kind,"track_id":id,"end":"end"}) } else { json!({"kind":kind,"via_id":id}) };
    let start = input_terminal("track_end", t.id);
    let target = input_terminal("via", v.id);
    project.board.tracks.insert(t.id, t);
    project.board.vias.insert(v.id, v);
    let engine = EngineHandle::new(project);
    let reg = engine_registry(engine.clone()).unwrap();
    let read = reg.clone().read_only();
    let a = CallContext::new("terminal-a");
    let b = CallContext::new("terminal-b");
    let before = engine.session_state();
    let status = engine.document_status();
    let rows = read.call("query_route_terminals", json!({}), b.clone()).await.unwrap();
    assert_eq!(rows["total"], 4);
    assert_eq!(rows["source_cursor"], serde_json::to_value(before.cursor).unwrap());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    for command in ["route_terminal_polyline", "route_terminal_posture"] {
        let mut input = json!({"expected_project":before.project.id,"expected_cursor":engine.event_cursor(),"start":start,"target":target,"layer":"top","width":250000,"grid":250000});
        if command == "route_terminal_posture" {
            input["mode"] = json!("cardinal");
            input["posture"] = json!("alternate");
        }
        let unchanged = engine.session_state();
        assert!(read.call(command, input.clone(), b.clone()).await.is_err());
        assert_eq!(engine.session_state(), unchanged);
        let result = reg.call(command, input.clone(), a.clone()).await.unwrap();
        assert!(!result["created"].as_array().unwrap().is_empty());
        let after = engine.read(Clone::clone);
        let observed = reg.call("get_session_state", json!({}), b.clone()).await.unwrap();
        assert_eq!(observed["project"], serde_json::to_value(&after).unwrap());
        assert!(reg.call(command, input, b.clone()).await.is_err());
        assert_eq!(engine.read(Clone::clone), after);
        reg.call("undo", json!({}), a.clone()).await.unwrap();
        assert_eq!(engine.read(Clone::clone), before.project);
        reg.call("redo", json!({}), a.clone()).await.unwrap();
        assert_eq!(engine.read(Clone::clone), after);
        reg.call("undo", json!({}), a.clone()).await.unwrap();
    }
}
