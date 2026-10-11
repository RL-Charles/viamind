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
async fn two_real_clients_share_detached_files_guarded_placement_and_scoped_undo() {
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap());
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("symbol-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("symbol-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    assert_eq!(
        serde_json::to_value(&tools.iter().find(|t| t.name == "load_symbol_template").unwrap().input_schema).unwrap(),
        viamind_engine::input_schema::<viamind_engine::LoadSymbolTemplate>()
    );
    let before = engine.session_state();
    let sheet = before.project.schematic.sheets.values().next().unwrap();
    let symbol = sheet.content.as_ref().unwrap().symbols.values().find(|s| s.designator == "U1").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.symbol.json");
    let export = json!({"expected_project":before.project.id,"source_cursor":before.cursor,"sheet_id":sheet.id,"symbol_id":symbol.id,"name":"Original regulator","path":path});
    assert_eq!(call(&a, "export_symbol_template", export.clone()).await.is_error, Some(false));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(call(&b, "export_symbol_template", export).await.is_error, Some(true));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let template = call(&b, "load_symbol_template", json!({"path":path})).await.structured_content.unwrap();
    let guard = call(&a, "snapshot_symbol_sheet", json!({"sheet_id":sheet.id})).await.structured_content.unwrap();
    assert_eq!(engine.session_state(), before);
    let mut command = json!({"expected_project":guard["expected_project"],"source_cursor":guard["source_cursor"],"expected":guard["expected"],"template":template,"designator":"U2","position":{"x":40000001,"y":50000003},"rotation_quarters":1,"mirrored":true});
    assert_eq!(call(&b, "add_part", json!({"designator":"R99","value":"peer","footprint":"F","x_mm":5,"y_mm":5})).await.is_error, Some(false));
    let peer = engine.session_state();
    assert_eq!(call(&a, "place_symbol_template", command.clone()).await.is_error, Some(true));
    assert_eq!(engine.session_state(), peer);
    let guard = call(&a, "snapshot_symbol_sheet", json!({"sheet_id":sheet.id})).await.structured_content.unwrap();
    command["source_cursor"] = guard["source_cursor"].clone();
    command["expected"] = guard["expected"].clone();
    assert_eq!(call(&a, "place_symbol_template", command).await.is_error, Some(false));
    let created = engine.read(Clone::clone);
    assert_eq!(created.board, peer.project.board);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), peer.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), created);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    for (tool, args) in [
        ("load_symbol_template", json!({"path":"/missing/viamind-symbol"})),
        ("load_symbol_template", json!({"path":"x","unknown":true})),
        ("snapshot_symbol_sheet", json!({"sheet_id":symbol.id})),
    ] {
        let state = engine.session_state();
        assert_eq!(call(&b, tool, args).await.is_error, Some(true));
        assert_eq!(engine.session_state(), state);
    }
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
