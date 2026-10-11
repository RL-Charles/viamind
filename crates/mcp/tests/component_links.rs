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
async fn two_actual_http_clients_query_stable_links_across_peer_rename_unlink_and_undo() {
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
    assert!(tools.iter().any(|t| t.name == "query_linked_components"));
    let before = engine.session_state();
    let status = engine.document_status();
    let result = call(&a, "query_linked_components", json!({})).await.structured_content.unwrap();
    assert_eq!(result["total"], 3);
    assert_eq!(call(&b, "query_linked_components", json!({})).await.structured_content.unwrap(), result);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let row = &result["components"][0];
    let part = row["part_id"].clone();
    let symbol = row["symbol_id"].clone();
    assert_eq!(call(&b, "update_part", json!({"id":part,"designator":"PEER1"})).await.is_error, Some(false));
    let query = json!({"object_ids":[symbol]});
    let renamed = call(&a, "query_linked_components", query.clone()).await.structured_content.unwrap();
    assert_eq!(renamed["components"][0]["part_id"], part);
    assert_eq!(renamed["components"][0]["part_reference"], "PEER1");
    assert_ne!(renamed["source_cursor"], result["source_cursor"]);
    let snapshot = engine.session_state();
    let id: viamind_doc::ObjectId = serde_json::from_value(part).unwrap();
    assert_eq!(
        call(
            &b,
            "link_schematic_symbol",
            json!({"expected_project":snapshot.project.id,"expected_part":snapshot.project.board.parts[&id],"target":{"kind":"unlink"}})
        )
        .await
        .is_error,
        Some(false)
    );
    let unlinked = call(&a, "query_linked_components", query.clone()).await.structured_content.unwrap();
    assert_eq!(unlinked["components"][0]["status"], "unlinked_symbol");
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(call(&a, "query_linked_components", query).await.structured_content.unwrap()["components"][0]["status"], "linked");
    let checkpoint = engine.session_state();
    assert_eq!(call(&a, "query_linked_components", json!({"limit":0})).await.is_error, Some(true));
    assert_eq!(engine.session_state(), checkpoint);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
