use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn client(url: String) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new("same branch client", "0"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(c: &Client, name: &str, input: Value) -> rmcp::model::CallToolResult {
    c.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
fn value(r: rmcp::model::CallToolResult) -> Value {
    assert_eq!(r.is_error, Some(false));
    r.structured_content.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn actual_two_connections_own_branches_merge_exact_ids_and_read_only_hides_all_writers() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("native.json");
    viamind_doc::file::save(&viamind_doc::Project::new("MCP sandbox"), &path).unwrap();
    let native = std::fs::read(&path).unwrap();
    let e = EngineHandle::load(&path).unwrap();
    let before = e.session_state();
    let status = e.document_status();
    let registry = engine_registry(e.clone()).unwrap();
    let server = serve_http(registry.clone(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = client(server.url()).await;
    let b = client(server.url()).await;
    let branch = value(
        call(&a, "create_pcb_branch", json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"label":"private proposal"})).await,
    );
    let id = branch["branch_id"].clone();
    let input = json!({"branch_id":id,"expected_revision":0,"commands":[{"name":"add_part","input":{"designator":"U1","value":"authored","footprint":"authored","x_mm":1.000003,"y_mm":-2.000007}}]});
    assert_eq!(call(&b, "apply_pcb_branch", input.clone()).await.is_error, Some(true));
    assert_eq!(call(&b, "inspect_pcb_branch", json!({"branch_id":id})).await.is_error, Some(true));
    let applied = value(call(&a, "apply_pcb_branch", input).await);
    let created: viamind_doc::ObjectId = serde_json::from_value(applied["created"][0].clone()).unwrap();
    let diff = value(call(&b, "get_pcb_branch_diff", json!({"branch_id":id})).await);
    assert_eq!(diff["total"], 1);
    assert_eq!(diff["deltas"][0]["after"]["id"], json!(created));
    assert_eq!(diff["deltas"][0]["after"]["position"]["x"], 1_000_003);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    assert_eq!(std::fs::read(&path).unwrap(), native);
    // Observe a nonempty private candidate through the actual restricted transport.
    let restricted = serve_http(registry.read_only(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let read = client(restricted.url()).await;
    let tools = read.list_all_tools().await.unwrap();
    for allowed in ["list_pcb_branches", "get_pcb_branch_diff"] {
        assert!(tools.iter().any(|t| t.name == allowed));
    }
    assert_eq!(value(call(&read, "get_pcb_branch_diff", json!({"branch_id":id})).await), diff);
    let listed = call(&read, "list_pcb_branches", json!({})).await;
    assert_eq!(listed.is_error, Some(false));
    let listed: Value = serde_json::from_str(&listed.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["branch_id"], id);
    for blocked in ["create_pcb_branch", "apply_pcb_branch", "inspect_pcb_branch", "merge_pcb_branch", "discard_pcb_branch"] {
        assert!(!tools.iter().any(|t| t.name == blocked));
        assert!(read.call_tool(CallToolRequestParams::new(blocked.to_owned()).with_arguments(serde_json::Map::new())).await.is_err());
    }
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    assert_eq!(std::fs::read(&path).unwrap(), native);
    let merging = json!({"branch_id":id,"expected_revision":1,"expected_project":before.project.id,"expected_cursor":before.cursor});
    assert_eq!(call(&b, "merge_pcb_branch", merging.clone()).await.is_error, Some(true));
    value(call(&a, "merge_pcb_branch", merging.clone()).await);
    assert!(e.read(|p| p.board.parts.contains_key(&created)));
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    assert_eq!(call(&a, "merge_pcb_branch", merging).await.is_error, Some(true));
    value(call(&a, "undo", json!({})).await);
    assert_eq!(e.read(Clone::clone), before.project);
    value(call(&a, "redo", json!({})).await);
    assert!(e.read(|p| p.board.parts.contains_key(&created)));
    let after = e.session_state();
    assert_eq!(e.session_state(), after);
    assert_eq!(std::fs::read(&path).unwrap(), native);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    read.cancel().await.unwrap();
    server.shutdown();
    restricted.shutdown();
}
