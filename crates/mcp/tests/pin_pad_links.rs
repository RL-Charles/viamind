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
async fn two_actual_http_clients_query_terminal_pairs_across_peer_edit_unlink_and_undo() {
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("probe-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("probe-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "query_linked_pin_pads"));
    let id = engine.read(|p| *p.board.parts.keys().next().unwrap());
    let args = json!({"component_id":id});
    let before = engine.session_state();
    let status = engine.document_status();
    let result = call(&a, "query_linked_pin_pads", args.clone()).await.structured_content.unwrap();
    assert_eq!(result["coverage"], "linked");
    assert_eq!(result["total"], 3);
    assert_eq!(call(&b, "query_linked_pin_pads", args.clone()).await.structured_content.unwrap(), result);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(call(&b, "update_part", json!({"id":id,"value":"peer"})).await.is_error, Some(false));
    let renamed = call(&a, "query_linked_pin_pads", args.clone()).await.structured_content.unwrap();
    assert_eq!(renamed["terminals"], result["terminals"]);
    assert_ne!(renamed["source_cursor"], result["source_cursor"]);
    let before = engine.session_state();
    assert_eq!(call(&a, "query_linked_pin_pads", json!({"component_id":id,"limit":0})).await.is_error, Some(true));
    assert_eq!(engine.session_state(), before);
    let part = before.project.board.parts[&id].clone();
    assert_eq!(
        call(&b, "link_schematic_symbol", json!({"expected_project":before.project.id,"expected_part":part,"target":{"kind":"unlink"}}))
            .await
            .is_error,
        Some(false)
    );
    assert_eq!(call(&a, "query_linked_pin_pads", args.clone()).await.structured_content.unwrap()["coverage"], "unlinked");
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(call(&a, "query_linked_pin_pads", args).await.structured_content.unwrap()["coverage"], "linked");
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
