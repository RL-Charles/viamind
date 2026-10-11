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

#[test]
fn borrowed_definition_limits_precede_typed_row_decoding_and_preserve_session() {
    use viamind_mcp::{agent::AgentId, engine_host::EngineHost, host::CommandHost};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("never-created.json");
    let engine = EngineHandle::new(viamind_doc::Project::new("unchanged"));
    let host = EngineHost::new(engine.clone());
    let before = engine.session_state();
    let status = engine.document_status();
    let agent = AgentId("bounded-definition".into());
    // Null rows deliberately cannot deserialize: count must reject first.
    let input = json!({"path":path,"footprint":{"name":"Explicit","pads":vec![Value::Null;1025]}});
    assert!(host.execute("save_footprint_definition", input, &agent).unwrap_err().contains("at most 1024 pads before typed decoding"));
    let oversized_name = json!({"path":path,"footprint":{"name":"a".repeat(513),"pads":[null]}});
    assert!(host.execute("save_footprint_definition", oversized_name, &agent).unwrap_err().contains("512 UTF-8 bytes before typed decoding"));
    let oversized_number = json!({"path":path,"footprint":{"name":"Explicit","pads":[{"number":"a".repeat(513)}]}});
    assert!(host.execute("save_footprint_definition", oversized_number, &agent).unwrap_err().contains("512 UTF-8 bytes before typed decoding"));
    for command in ["load_footprint_definition", "save_footprint_definition"] {
        assert!(host.execute(command, json!({"path":"a".repeat(4097)}), &agent).unwrap_err().contains("4096 UTF-8 bytes"));
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert!(!path.exists());
}

async fn call(client: &Client, name: &'static str, input: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn real_two_clients_share_detached_files_with_separate_guarded_reuse_and_no_implicit_design_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new-footprint.json");
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("definition-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("definition-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    for tool in ["load_footprint_definition", "save_footprint_definition"] {
        assert!(a.list_all_tools().await.unwrap().iter().any(|t| t.name == tool));
    }
    let id = call(&b, "add_part", json!({"designator":"U1","value":"dirty","footprint":"old","x_mm":1.000003,"y_mm":2.000007}))
        .await
        .structured_content
        .unwrap()["created"][0]
        .clone();
    let original = engine.read(|p| p.board.parts.values().next().unwrap().clone());
    assert_eq!(
        call(
            &b,
            "set_part_pads",
            json!({"id":id,"expected":original,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":"VIN"}]})
        )
        .await
        .is_error,
        Some(false)
    );
    let before = engine.session_state();
    let status = engine.document_status();
    let footprint = json!({"name":"Original odd Nm","pads":[{"number":"1","position":{"x":-500003,"y":250001},"width":1200003,"height":800001},{"number":"2","position":{"x":750007,"y":-125003},"width":600003,"height":1000001}]});
    let input = json!({"footprint":footprint,"path":path});
    assert_eq!(call(&a, "save_footprint_definition", input.clone()).await.is_error, Some(false));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(call(&b, "load_footprint_definition", json!({"path":path})).await.structured_content.unwrap(), footprint);
    assert_eq!(call(&b, "save_footprint_definition", input.clone()).await.is_error, Some(true));
    assert_eq!(call(&a, "load_footprint_definition", json!({"path":path,"extra":true})).await.is_error, Some(true));
    let mut invalid = input.clone();
    invalid["footprint"]["pads"][1]["number"] = json!("1");
    invalid["path"] = json!(directory.path().join("invalid.json"));
    assert_eq!(call(&a, "save_footprint_definition", invalid).await.is_error, Some(true));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let expected = before.project.board.parts.values().next().unwrap();
    let apply = json!({"expected_project":before.project.id,"expected":expected,"path":path});
    assert_eq!(call(&a, "import_footprint", apply.clone()).await.is_error, Some(false));
    let after = engine.read(|p| p.board.part(&expected.id).unwrap().clone());
    assert_eq!(after.position, expected.position);
    assert_eq!(after.pads.as_ref().unwrap().0[0].id, expected.pads.as_ref().unwrap().0[0].id);
    assert_eq!(after.pads.as_ref().unwrap().0[0].net.as_deref(), Some("VIN"));
    assert!(after.pads.as_ref().unwrap().0[1].net.is_none());
    assert_eq!(call(&b, "import_footprint", apply).await.is_error, Some(true));
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.session_state().project, before.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(|p| p.board.part(&expected.id).unwrap().clone()), after);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
