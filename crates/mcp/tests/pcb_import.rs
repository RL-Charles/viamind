use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
const SOURCE: &str = include_str!("../../../fixtures/import/original-two-layer.kicad_pcb");
async fn call(client: &Client, name: &'static str, input: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
fn apply(preview: &Value) -> Value {
    json!({"path":preview["path"],"expected_project":preview["expected_project"],"expected_cursor":preview["expected_cursor"],"expected_source_sha256":preview["source_sha256"]})
}
#[tokio::test(flavor = "multi_thread")]
async fn two_actual_clients_preview_exact_source_and_protect_dirty_stale_and_native_sessions() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_pcb");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("import-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("import-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    for tool in ["preview_pcb_import", "import_pcb"] {
        assert!(a.list_all_tools().await.unwrap().iter().any(|t| t.name == tool));
    }
    let before = engine.session_state();
    let status = engine.document_status();
    let preview = call(&a, "preview_pcb_import", json!({"path":source})).await.structured_content.unwrap();
    assert_eq!(preview["can_import"], true);
    assert_eq!(call(&b, "preview_pcb_import", json!({"path":source})).await.structured_content.unwrap(), preview);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(call(&b, "preview_pcb_import", json!({"path":source,"extra":true})).await.is_error, Some(true));
    assert_eq!(call(&b, "add_part", json!({"designator":"KEEP1","value":"dirty","footprint":"test","x_mm":0,"y_mm":0})).await.is_error, Some(false));
    let dirty = engine.session_state();
    assert_eq!(call(&a, "import_pcb", apply(&preview)).await.is_error, Some(true));
    let fresh = call(&a, "preview_pcb_import", json!({"path":source})).await.structured_content.unwrap();
    assert_eq!(call(&b, "import_pcb", apply(&fresh)).await.is_error, Some(true));
    assert_eq!(engine.session_state(), dirty);
    let existing = directory.path().join("keep.viamind.json");
    assert_eq!(call(&b, "save_project", json!({"path":existing})).await.is_error, Some(false));
    let saved = engine.session_state();
    let saved_status = engine.document_status();
    std::fs::write(&source, format!("{SOURCE}\n")).unwrap();
    assert_eq!(call(&b, "import_pcb", apply(&fresh)).await.is_error, Some(true));
    assert_eq!(engine.session_state(), saved);
    assert_eq!(engine.document_status(), saved_status);
    std::fs::write(&source, SOURCE).unwrap();
    let imported = call(&b, "import_pcb", apply(&fresh)).await.structured_content.unwrap();
    assert_eq!(imported["document"]["dirty"], true);
    assert!(imported["document"]["path"].is_null());
    assert_ne!(imported["cursor"]["stream_id"], fresh["expected_cursor"]["stream_id"]);
    let project = engine.session_state().project;
    assert_eq!(project.board.parts.len(), 1);
    assert_eq!(project.board.tracks.len(), 2);
    assert_eq!(project.board.vias.len(), 1);
    assert_eq!(call(&a, "import_pcb", apply(&fresh)).await.is_error, Some(true));
    assert_eq!(engine.session_state().project, project);
    let native = directory.path().join("imported.viamind.json");
    assert_eq!(call(&a, "save_project", json!({"path":native})).await.is_error, Some(false));
    assert_eq!(call(&b, "open_project", json!({"path":native,"expected_cursor":engine.event_cursor()})).await.is_error, Some(false));
    assert_eq!(engine.session_state().project, project);
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    assert_eq!(viamind_doc::file::load(&existing).unwrap(), saved.project);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown().await.unwrap();
}
