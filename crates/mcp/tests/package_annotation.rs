use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::{AnnotationOptions, EngineHandle};
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn actual_two_clients_group_owner_preview_apply_cursor_and_atomic_undo() {
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/package-annotation.viamind.json")).unwrap());
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("package-annotation-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("package-annotation-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    assert_eq!(
        serde_json::to_value(&tools.iter().find(|t| t.name == "preview_schematic_annotation").unwrap().input_schema).unwrap(),
        viamind_engine::input_schema::<AnnotationOptions>()
    );
    assert!(tools.iter().any(|t| t.name == "annotate_schematic"));
    let state = engine.session_state();
    let options = json!({"mode":"reset_all","start":10});
    let preview = call(&a, "preview_schematic_annotation", options.clone()).await.structured_content.unwrap();
    assert_eq!(preview["changes"].as_array().unwrap().len(), 12);
    assert_eq!(call(&b, "preview_schematic_annotation", options.clone()).await.structured_content.unwrap(), preview);
    assert_eq!(preview["symbols"], 12);
    assert_eq!(preview["packages"], 2);
    assert_eq!(preview["command"]["expected_cursor"], serde_json::to_value(state.cursor).unwrap());
    let mut missing = preview["command"].clone();
    missing.as_object_mut().unwrap().remove("expected_cursor");
    assert_eq!(call(&b, "annotate_schematic", missing).await.is_error, Some(true));
    assert_eq!(engine.session_state(), state);
    for bad in [json!({"start":0}), json!({"start":1000000000u32}), json!({"mode":"other"}), json!({"unknown":true})] {
        assert_eq!(call(&b, "preview_schematic_annotation", bad).await.is_error, Some(true));
        assert_eq!(engine.session_state(), state);
    }
    let sheet = state.project.schematic.sheets.values().next().unwrap();
    assert_eq!(
        call(
            &b,
            "edit_schematic_sheet",
            json!({"expected_project":state.project.id,"sheet_id":sheet.id,"expected":sheet,"name":"Agent rename","edits":[]})
        )
        .await
        .is_error,
        Some(false)
    );
    let fresh = engine.session_state();
    assert_eq!(call(&a, "annotate_schematic", preview["command"].clone()).await.is_error, Some(true));
    assert_eq!(engine.session_state(), fresh);
    let preview = call(&a, "preview_schematic_annotation", options).await.structured_content.unwrap();
    assert_eq!(call(&a, "annotate_schematic", preview["command"].clone()).await.is_error, Some(false));
    let changed = engine.session_state();
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(true));
    assert_eq!(engine.session_state(), changed);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), fresh.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), changed.project);
    assert_eq!(engine.read(|p| p.board.clone()), state.project.board);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
