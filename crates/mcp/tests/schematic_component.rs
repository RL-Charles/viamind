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
async fn call(c: &Client, name: &'static str, args: Value) -> CallToolResult {
    c.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn two_real_agents_preview_shared_creation_stale_conflict_own_undo_and_native_identity() {
    let e = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap());
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("create-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("create-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    for name in ["preview_schematic_pcb_component", "create_schematic_pcb_component"] {
        assert_eq!(tools.iter().find(|t| t.name == name).unwrap().input_schema["type"], "object");
    }
    let checkpoint = e.session_state();
    let symbol = checkpoint
        .project
        .schematic
        .sheets
        .values()
        .next()
        .unwrap()
        .content
        .as_ref()
        .unwrap()
        .symbols
        .values()
        .find(|s| s.designator == "U1")
        .unwrap();
    let footprint = viamind_doc::footprint::from_slice(include_bytes!("../../../fixtures/footprints/original-smd3.viamind-footprint.json")).unwrap();
    let mut request = json!({"expected_project":checkpoint.project.id,"source_cursor":checkpoint.cursor,"placement":{"symbol_id":symbol.id,"footprint":footprint,"position":{"x":40000001,"y":50000003},"side":"bottom","rotation_mdeg":90000}});
    let preview = call(&a, "preview_schematic_pcb_component", request.clone()).await.structured_content.unwrap();
    assert_eq!(preview["pads"].as_array().unwrap().len(), 3);
    assert_eq!(call(&b, "preview_schematic_pcb_component", request.clone()).await.structured_content.unwrap(), preview);
    assert_eq!(e.session_state(), checkpoint);
    assert_eq!(call(&b, "add_part", json!({"designator":"R99","value":"agent","footprint":"other","x_mm":2,"y_mm":3})).await.is_error, Some(false));
    let concurrent = e.session_state();
    assert_eq!(call(&a, "create_schematic_pcb_component", preview["command"].clone()).await.is_error, Some(true));
    assert_eq!(e.session_state(), concurrent);
    request["source_cursor"] = json!(concurrent.cursor);
    let preview = call(&a, "preview_schematic_pcb_component", request).await.structured_content.unwrap();
    assert_eq!(call(&a, "create_schematic_pcb_component", preview["command"].clone()).await.is_error, Some(false));
    let after = e.session_state();
    assert_eq!(after.cursor.sequence, concurrent.cursor.sequence + 1);
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(false));
    let mut only_created = after.project.clone();
    only_created.board.parts.retain(|_, part| part.designator != "R99");
    assert_eq!(e.read(Clone::clone), only_created, "peer undoes its unrelated part while preserving the linked creation");
    assert_eq!(call(&b, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), after.project);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), concurrent.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), after.project);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("created.json");
    assert_eq!(call(&a, "save_project", json!({"path":path})).await.is_error, Some(false));
    assert_eq!(viamind_doc::file::load(&path).unwrap(), after.project);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
