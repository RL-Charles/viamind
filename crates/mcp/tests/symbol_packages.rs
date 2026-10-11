use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::{Project, Sheet, schematic::SchematicContent};
use viamind_engine::{EngineHandle, PlaceSymbolPackage, UnitPose};
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn connect(url: String, name: &str) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new(name, "1"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(c: &Client, name: &'static str, args: Value) -> CallToolResult {
    c.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn two_clients_share_complete_package_identity_atomic_history_and_safe_detached_files() {
    let mut p = Project::new("Package parity");
    let mut sheet = Sheet::new("Units");
    sheet.content = Some(SchematicContent::default());
    let sheet_id = sheet.id;
    p.schematic.sheets.insert(sheet_id, sheet);
    let e = EngineHandle::new(p);
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "package-a").await;
    let b = connect(server.url(), "package-b").await;
    let tools = a.list_all_tools().await.unwrap();
    for name in ["load_symbol_package", "save_symbol_package", "place_symbol_package", "delete_symbol_package", "query_symbol_packages"] {
        assert!(tools.iter().any(|t| t.name == name), "missing {name}");
    }
    assert_eq!(
        serde_json::to_value(&tools.iter().find(|t| t.name == "place_symbol_package").unwrap().input_schema).unwrap(),
        viamind_engine::input_schema::<PlaceSymbolPackage>()
    );
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json");
    let source_bytes = std::fs::read(&source).unwrap();
    let before = e.session_state();
    let status = e.document_status();
    let template = call(&a, "load_symbol_package", json!({"path":source})).await.structured_content.unwrap();
    assert_eq!(call(&b, "load_symbol_package", json!({"path":source})).await.structured_content.unwrap(), template);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new-package.json");
    let save = json!({"template":template,"path":path});
    assert_eq!(call(&a, "save_symbol_package", save.clone()).await.is_error, Some(false));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(call(&b, "save_symbol_package", save).await.is_error, Some(true));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    for (units, needle) in [(vec![Value::Null; 33], "2..32"), (vec![json!({"pins":vec![Value::Null;1025]}), json!({"pins":[null]})], "1024")] {
        let target = dir.path().join("never.json");
        let result = call(&b, "save_symbol_package", json!({"path":target,"template":{"units":units}})).await;
        assert_eq!(result.is_error, Some(true));
        assert!(serde_json::to_string(&result).unwrap().contains(needle));
        assert!(!target.exists());
    }
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    let guard = e.snapshot_symbol_sheet(&viamind_engine::SymbolSheetQuery { sheet_id }).unwrap();
    let definition = viamind_doc::symbol_package::from_slice(&source_bytes).unwrap();
    let placements = definition
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| UnitPose {
            key: u.key.clone(),
            position: serde_json::from_value(json!({"x":i as i64 * 20_000_000 + 1,"y":3})).unwrap(),
            rotation_quarters: (i % 4) as u8,
            mirrored: i % 2 == 1,
        })
        .collect();
    let mut command = serde_json::to_value(PlaceSymbolPackage {
        expected_project: guard.expected_project,
        source_cursor: guard.source_cursor,
        expected: guard.expected,
        template: definition,
        designator: "U1".into(),
        placements,
    })
    .unwrap();
    assert_eq!(call(&b, "add_part", json!({"designator":"PEER1","value":"keep","footprint":"F","x_mm":1,"y_mm":2})).await.is_error, Some(false));
    let peer = e.session_state();
    assert_eq!(call(&a, "place_symbol_package", command.clone()).await.is_error, Some(true));
    assert_eq!(e.session_state(), peer);
    let guard = call(&a, "snapshot_symbol_sheet", json!({"sheet_id":sheet_id})).await.structured_content.unwrap();
    command["source_cursor"] = guard["source_cursor"].clone();
    command["expected"] = guard["expected"].clone();
    assert_eq!(call(&a, "place_symbol_package", command).await.is_error, Some(false));
    let placed = e.session_state();
    let report = call(&a, "query_symbol_packages", json!({})).await.structured_content.unwrap();
    assert_eq!(call(&b, "query_symbol_packages", json!({})).await.structured_content.unwrap(), report);
    assert_eq!(report["total"], 1);
    let group = &report["packages"][0];
    assert_eq!(group["units"].as_array().unwrap().iter().map(|u| u["pins"].as_array().unwrap().len()).sum::<usize>(), 14);
    assert_eq!(group["units"][4]["role"], "power");
    assert_eq!(group["units"][4]["pins"][0]["pin"]["number"], "13");
    assert_eq!(e.session_state(), placed);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), peer.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), placed.project);
    let guard = call(&a, "snapshot_symbol_sheet", json!({"sheet_id":sheet_id})).await.structured_content.unwrap();
    assert_eq!(call(&a,"delete_symbol_package",json!({"expected_project":guard["expected_project"],"source_cursor":guard["source_cursor"],"expected":guard["expected"],"package_id":group["package_id"]})).await.is_error,Some(false));
    assert_eq!(e.read(Clone::clone), peer.project);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(e.read(Clone::clone), placed.project);
    let state = e.session_state();
    let state_status = e.document_status();
    let readonly = serve_http(engine_registry(e.clone()).unwrap().read_only(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let observer = connect(readonly.url(), "observer").await;
    let names = observer.list_all_tools().await.unwrap();
    for name in ["query_symbol_packages", "load_symbol_package"] {
        assert!(names.iter().any(|t| t.name == name));
    }
    for name in ["save_symbol_package", "place_symbol_package", "delete_symbol_package"] {
        assert!(!names.iter().any(|t| t.name == name));
        assert!(observer.call_tool(CallToolRequestParams::new(name).with_arguments(serde_json::Map::new())).await.is_err());
    }
    assert_eq!(call(&observer, "load_symbol_package", json!({"path":source})).await.structured_content.unwrap(), template);
    assert_eq!(call(&observer, "query_symbol_packages", json!({})).await.is_error, Some(false));
    assert_eq!(e.session_state(), state);
    assert_eq!(e.document_status(), state_status);
    assert_eq!(std::fs::read(source).unwrap(), source_bytes);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    observer.cancel().await.unwrap();
    readonly.shutdown();
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
