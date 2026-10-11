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
const SOURCE: &str = include_str!("../../../fixtures/import/original-cardinal-pads.kicad_mod");
async fn call(client: &Client, name: &'static str, input: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn actual_two_clients_convert_without_project_changes_and_reuse_only_by_explicit_number_mapping() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_mod");
    std::fs::write(&source, SOURCE).unwrap();
    let output = directory.path().join("converted.viamind-footprint.json");
    let native = directory.path().join("keep.viamind.json");
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    engine.save_project(viamind_engine::SaveProject { path: Some(native.clone()), overwrite: false }).unwrap();
    let native_bytes = std::fs::read(&native).unwrap();
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("conversion-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("conversion-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    for tool in ["preview_footprint_conversion", "convert_footprint_file"] {
        assert!(a.list_all_tools().await.unwrap().iter().any(|t| t.name == tool));
    }
    let id = call(&b, "add_part", json!({"designator":"KEEP1","value":"unsaved","footprint":"old","x_mm":3,"y_mm":4}))
        .await
        .structured_content
        .unwrap()["created"][0]
        .clone();
    let expected = serde_json::to_value(engine.read(|p| p.board.part(&serde_json::from_value(id.clone()).unwrap()).unwrap().clone())).unwrap();
    assert_eq!(
        call(
            &b,
            "set_part_pads",
            json!({"id":id,"expected":expected,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":"VIN"}]})
        )
        .await
        .is_error,
        Some(false)
    );
    let before = engine.session_state();
    let status = engine.document_status();
    assert!(status.dirty);
    let preview = call(&a, "preview_footprint_conversion", json!({"path":source})).await.structured_content.unwrap();
    assert_eq!(preview["can_convert"], true);
    assert_eq!(preview["report"]["full_fidelity"], false);
    assert_eq!(call(&b, "preview_footprint_conversion", json!({"path":source})).await.structured_content.unwrap(), preview);
    assert_eq!(call(&a, "preview_footprint_conversion", json!({"path":source,"extra":true})).await.is_error, Some(true));
    let input = json!({"path":source,"expected_source_sha256":preview["source_sha256"],"destination":output});
    std::fs::write(&source, format!("{SOURCE}\n")).unwrap();
    assert_eq!(call(&b, "convert_footprint_file", input.clone()).await.is_error, Some(true));
    assert!(!output.exists());
    std::fs::write(&source, SOURCE).unwrap();
    assert_eq!(call(&b, "convert_footprint_file", input.clone()).await.is_error, Some(false));
    assert_eq!(call(&a, "convert_footprint_file", input).await.is_error, Some(true));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let footprint = viamind_doc::footprint::load(&output).unwrap();
    assert_eq!(footprint.pads.len(), 2);
    assert_eq!((footprint.pads[0].width.0, footprint.pads[0].height.0), (1_200_003, 800_001));
    assert!(
        preview["report"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["reason"].as_str().unwrap().contains("source angle 90 degrees, canonical 90 degrees"))
    );
    let old = before.project.board.part(&serde_json::from_value(id.clone()).unwrap()).unwrap();
    assert_eq!(call(&a, "import_footprint", json!({"expected_project":before.project.id,"expected":old,"path":output})).await.is_error, Some(false));
    let applied = engine.read(|p| p.board.part(&old.id).unwrap().clone());
    let old_pad = &old.pads.as_ref().unwrap().0[0];
    let pads = &applied.pads.as_ref().unwrap().0;
    assert_eq!(pads[0].id, old_pad.id);
    assert_eq!(pads[0].net.as_deref(), Some("VIN"));
    assert_ne!(pads[1].id, old_pad.id);
    assert!(pads[1].net.is_none());
    assert_eq!(applied.position, old.position);
    assert_eq!(
        call(&b, "convert_footprint_file", json!({"path":source,"expected_source_sha256":preview["source_sha256"],"destination":native}))
            .await
            .is_error,
        Some(true)
    );
    assert_eq!(std::fs::read(&native).unwrap(), native_bytes);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), SOURCE);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
