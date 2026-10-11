use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::{Project, Sheet};
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn call(c: &Client, name: &'static str, args: Value) -> CallToolResult {
    c.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn two_real_agents_snapshot_stale_atomic_path_and_own_undo() {
    let mut project = Project::new("wire HTTP");
    let sheet = Sheet::new("Main");
    let sheet_id = sheet.id;
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("wires-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("wires-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    for name in ["snapshot_schematic_wire_sheet", "draw_schematic_wire_polyline"] {
        assert_eq!(tools.iter().find(|t| t.name == name).unwrap().input_schema["type"], "object");
    }
    let before = e.session_state();
    let mut cmd = call(&a, "snapshot_schematic_wire_sheet", json!({"sheet_id":sheet_id})).await.structured_content.unwrap();
    assert_eq!(cmd, call(&b, "snapshot_schematic_wire_sheet", json!({"sheet_id":sheet_id})).await.structured_content.unwrap());
    assert_eq!(before, e.session_state());
    cmd["vertices"] = json!([{"x":1,"y":3},{"x":7,"y":3},{"x":7,"y":11}]);
    assert_eq!(call(&b, "add_part", json!({"designator":"R1","value":"peer","footprint":"x","x_mm":1,"y_mm":1})).await.is_error, Some(false));
    let peer = e.session_state();
    assert_eq!(call(&a, "draw_schematic_wire_polyline", cmd).await.is_error, Some(true));
    assert_eq!(e.session_state(), peer);
    let mut cmd = call(&a, "snapshot_schematic_wire_sheet", json!({"sheet_id":sheet_id})).await.structured_content.unwrap();
    cmd["vertices"] = json!([{"x":1,"y":3},{"x":7,"y":3},{"x":7,"y":11}]);
    assert_eq!(call(&a, "draw_schematic_wire_polyline", cmd).await.is_error, Some(false));
    let after = e.session_state();
    assert_eq!(after.cursor.sequence, peer.cursor.sequence + 1);
    assert_eq!(after.project.schematic.sheets[&sheet_id].content.as_ref().unwrap().wires.len(), 2);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), peer.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), after.project);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
