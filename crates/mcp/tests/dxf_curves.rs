//! Two actual HTTP MCP clients share certified DXF preview and guarded Apply.
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::{EngineHandle, agent_policy::AgentMode};
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
const ROUNDED: &str = include_str!("../../doc/tests/fixtures/outline-rounded.dxf");
async fn call(client: &Client, name: &'static str, input: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
fn apply(source: &Value, preview: &Value) -> Value {
    let mut input = source.clone();
    input["expected_source_sha256"] = preview["source_sha256"].clone();
    input["expected_cursor"] = preview["expected_cursor"].clone();
    input["expected"] = preview["expected"].clone();
    input
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_preview_text_and_file_sources_and_share_guarded_one_delta_apply() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rounded.dxf");
    std::fs::write(&path, ROUNDED).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("DXF agents"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let connect = |name: &'static str| {
        ClientConfig::new(ClientCapabilities::default(), Implementation::new(name, "1")).serve(StreamableHttpClientTransport::from_uri(server.url()))
    };
    let a = connect("dxf-a").await.unwrap();
    let b = connect("dxf-b").await.unwrap();
    let tools = a.list_all_tools().await.unwrap();
    for (name, schema) in [
        ("preview_board_outline_dxf", viamind_engine::input_schema::<viamind_engine::PreviewBoardOutlineDxf>()),
        ("apply_board_outline_dxf", viamind_engine::input_schema::<viamind_engine::ApplyBoardOutlineDxf>()),
    ] {
        let tool = tools.iter().find(|t| t.name == name).unwrap();
        assert_eq!(serde_json::to_value(&tool.input_schema).unwrap(), schema);
    }
    assert!(tools.iter().any(|t| t.name == "import_board_outline_dxf"), "straight-only command remains");

    let before = engine.session_state();
    let status = engine.document_status();
    let by_text = json!({"text":ROUNDED,"layer":"Edge","tolerance_nm":10000});
    let by_path = json!({"path":path,"layer":"Edge","tolerance_nm":10000});
    let text_preview = call(&a, "preview_board_outline_dxf", by_text.clone()).await.structured_content.unwrap();
    let path_preview = call(&b, "preview_board_outline_dxf", by_path.clone()).await.structured_content.unwrap();
    assert_eq!(text_preview["source_sha256"], path_preview["source_sha256"]);
    assert_eq!(text_preview["outline"], path_preview["outline"]);
    assert_eq!(text_preview["report"], path_preview["report"]);
    assert_eq!((text_preview["source"].as_str(), path_preview["source"].as_str()), (Some("text"), Some("path")));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    for bad in [
        json!({"text":ROUNDED,"tolerance_nm":10000,"extra":true}),
        json!({"text":ROUNDED,"path":path,"tolerance_nm":10000}),
        json!({"text":ROUNDED,"layer":"Edge"}),
        json!({"text":ROUNDED,"layer":"Edge","tolerance_nm":9}),
        json!({"layer":"L".repeat(513),"text":ROUNDED,"tolerance_nm":10000}),
    ] {
        assert_eq!(call(&b, "preview_board_outline_dxf", bad).await.is_error, Some(true));
    }

    // Peer edit makes the first preview stale for every client.
    engine.execute("add_part", json!({"designator":"R9","value":"peer","footprint":"X","x_mm":3,"y_mm":-3}), viamind_engine::Origin::User).unwrap();
    let peer = engine.session_state();
    assert_eq!(call(&a, "apply_board_outline_dxf", apply(&by_text, &text_preview)).await.is_error, Some(true));
    assert_eq!(engine.session_state(), peer);
    let fresh = call(&a, "preview_board_outline_dxf", by_path.clone()).await.structured_content.unwrap();
    // Changed file bytes after preview refuse by digest/reparse.
    std::fs::write(&path, ROUNDED.replace("40\n1.6", "40\n1.5")).unwrap();
    assert_eq!(call(&a, "apply_board_outline_dxf", apply(&by_path, &fresh)).await.is_error, Some(true));
    std::fs::write(&path, ROUNDED).unwrap();
    let applied = call(&a, "apply_board_outline_dxf", apply(&by_path, &fresh)).await.structured_content.unwrap();
    assert_eq!(applied["changed"], true);
    assert_eq!(applied["result"]["deltas"].as_array().unwrap().len(), 1);
    let after = engine.session_state();
    assert_eq!(serde_json::to_value(&after.project.board.outline).unwrap(), fresh["outline"]);
    assert_eq!(after.cursor.sequence, peer.cursor.sequence + 1);
    // The peer cannot apply the consumed preview, and cannot undo the creator's change.
    assert_eq!(call(&b, "apply_board_outline_dxf", apply(&by_path, &fresh)).await.is_error, Some(true));
    let peer_undo = b.call_tool(CallToolRequestParams::new("undo").with_arguments(Default::default())).await;
    assert!(peer_undo.is_err() || peer_undo.unwrap().is_error == Some(true));
    assert!(engine.read(|p| p.board.outline.is_some()), "peer undo must not reach creator history");
    // Equal geometry from a fresh preview is a no-op.
    let again = call(&b, "preview_board_outline_dxf", by_text.clone()).await.structured_content.unwrap();
    assert_eq!(again["changes_outline"], false);
    let no_op = call(&b, "apply_board_outline_dxf", apply(&by_text, &again)).await.structured_content.unwrap();
    assert_eq!(no_op["changed"], false);
    assert_eq!(engine.session_state(), after);

    // Restricted agent modes refuse Apply centrally; preview stays an observation.
    for mode in [AgentMode::ReadOnly, AgentMode::Approval] {
        engine.agent_policy().set_mode(mode).unwrap();
        let observed = call(&b, "preview_board_outline_dxf", by_text.clone()).await.structured_content.unwrap();
        let input = apply(&by_text, &observed);
        assert!(b.call_tool(CallToolRequestParams::new("apply_board_outline_dxf").with_arguments(input.as_object().unwrap().clone())).await.is_err());
        assert_eq!(engine.session_state(), after);
    }
    engine.agent_policy().set_mode(AgentMode::Live).unwrap();
    a.call_tool(CallToolRequestParams::new("undo").with_arguments(Default::default())).await.unwrap();
    assert!(engine.read(|p| p.board.outline.is_none()), "creator undo restores the previous outline");
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
