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
        serde_json::to_value(&tools.iter().find(|t| t.name == "save_symbol_template").unwrap().input_schema).unwrap(),
        viamind_engine::input_schema::<viamind_engine::SaveSymbolTemplate>()
    );
    assert_eq!(
        serde_json::to_value(&tools.iter().find(|t| t.name == "load_symbol_template").unwrap().input_schema).unwrap(),
        viamind_engine::input_schema::<viamind_engine::LoadSymbolTemplate>()
    );
    let before = engine.session_state();
    let status = engine.document_status();
    let scratch = tempfile::tempdir().unwrap();
    let no_output = scratch.path().join("never.json");
    let mut aggregate = vec![json!({"number":"","name":"a".repeat(4096)}); 64];
    aggregate.push(Value::Null);
    for (args, expected) in [
        (json!({"path":no_output,"template":{"name":"Explicit","pins":vec![Value::Null;1025]}}), "1024 pins before typed decoding"),
        (json!({"path":no_output,"template":{"name":"a".repeat(4097),"pins":[null]}}), "4096 UTF-8 bytes per field"),
        (json!({"path":no_output,"template":{"name":"Explicit","pins":[{"number":"a".repeat(4097)}]}}), "4096 UTF-8 bytes per field"),
        (json!({"path":no_output,"template":{"name":"x","pins":aggregate}}), "256 KiB aggregate"),
        (json!({"path":"a".repeat(4097)}), "4096 UTF-8 bytes before typed decoding"),
    ] {
        let result = call(&b, "save_symbol_template", args).await;
        assert_eq!(result.is_error, Some(true));
        assert!(serde_json::to_string(&result).unwrap().contains(expected), "{result:?}");
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
        assert!(!no_output.exists());
    }
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
    let detached = dir.path().join("edited.symbol.json");
    let mut edited = template.clone();
    edited["value"] = json!("Edited detached regulator");
    edited["width"] = json!(4_000_001);
    let save = json!({"template":edited,"path":detached});
    assert_eq!(call(&a, "save_symbol_template", save.clone()).await.is_error, Some(false));
    let saved_bytes = std::fs::read(&detached).unwrap();
    assert_eq!(call(&b, "save_symbol_template", save.clone()).await.is_error, Some(true));
    assert_eq!(std::fs::read(&detached).unwrap(), saved_bytes);
    assert_eq!(call(&b, "load_symbol_template", json!({"path":detached})).await.structured_content.unwrap(), edited);
    let mut invalid = save.clone();
    invalid["template"]["pins"][0]["number"] = invalid["template"]["pins"][1]["number"].clone();
    invalid["path"] = json!(dir.path().join("invalid.json"));
    assert_eq!(call(&b, "save_symbol_template", invalid).await.is_error, Some(true));
    assert!(!dir.path().join("invalid.json").exists());
    let mut unknown = save;
    unknown["extra"] = json!(true);
    assert_eq!(call(&b, "save_symbol_template", unknown).await.is_error, Some(true));
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
