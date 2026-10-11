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
async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn two_real_http_agents_preview_stale_rejection_atomic_update_native_persistence_and_own_undo() {
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("sync-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("sync-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    for name in ["preview_schematic_pcb_sync", "link_schematic_symbol", "synchronize_schematic_pcb"] {
        let schema = serde_json::to_value(&tools.iter().find(|t| t.name == name).unwrap().input_schema).unwrap();
        assert_eq!(schema["type"], "object");
    }
    let checkpoint = engine.session_state();
    let preview = call(&a, "preview_schematic_pcb_sync", json!({"direction":"forward"})).await.structured_content.unwrap();
    assert_eq!(preview["changes"].as_array().unwrap().len(), 13); // three references/values, seven pads
    assert_eq!(preview["nets"].as_array().unwrap().len(), 3);
    assert_eq!(call(&b, "preview_schematic_pcb_sync", json!({"direction":"forward"})).await.structured_content.unwrap(), preview);
    assert_eq!(engine.session_state(), checkpoint);
    let id = checkpoint.project.board.parts.keys().next().unwrap();
    assert_eq!(call(&b, "update_part", json!({"id":id,"value":"concurrent agent edit"})).await.is_error, Some(false));
    let concurrent = engine.session_state();
    assert_eq!(call(&a, "synchronize_schematic_pcb", preview["command"].clone()).await.is_error, Some(true));
    assert_eq!(engine.session_state(), concurrent);
    let preview = call(&a, "preview_schematic_pcb_sync", json!({"direction":"forward"})).await.structured_content.unwrap();
    assert_eq!(call(&a, "synchronize_schematic_pcb", preview["command"].clone()).await.is_error, Some(false));
    let after = engine.session_state();
    assert_eq!(after.cursor.sequence, concurrent.cursor.sequence + 1);
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(true));
    assert_eq!(engine.session_state(), after);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), concurrent.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), after.project);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("linked.viamind.json");
    assert_eq!(call(&a, "save_project", json!({"path":path})).await.is_error, Some(false));
    assert_eq!(viamind_doc::file::load(&path).unwrap(), after.project);
    assert_eq!(call(&a, "open_project", json!({"path":path})).await.is_error, Some(false));
    let reopened = engine.session_state();
    assert_eq!(call(&a, "synchronize_schematic_pcb", preview["command"].clone()).await.is_error, Some(true));
    assert_eq!(engine.session_state(), reopened, "same document UUID in a new stream invalidates the old command");
    let part = reopened.project.board.parts.values().next().unwrap();
    assert_eq!(
        call(&a, "link_schematic_symbol", json!({"expected_project":reopened.project.id,"expected_part":part,"target":{"kind":"unlink"}}))
            .await
            .is_error,
        Some(false)
    );
    assert_eq!(engine.read(|p| p.board.parts[&part.id].schematic_symbol), None);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), reopened.project);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
