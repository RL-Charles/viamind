use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::EngineHandle;
type Client = RunningService<RoleClient, ClientConfig>;
async fn connect(url: String, name: &str) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new(name, "1"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_real_readonly_clients_inspect_historical_release_without_document_or_native_changes() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/output-job-population.viamind.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    viamind_doc::file::save(&project, &native).unwrap();
    let native_bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let original = engine.session_state();
    let mut job = viamind_fab::OutputJob::new("Historical matched release".into(), 0.);
    job.version = 3;
    job.include_calibrated_placement = Some(true);
    job.include_populated_bom = Some(true);
    let release = temp.path().join("release");
    viamind_fab::OutputJobSnapshot::capture(&original.project, job).unwrap().write(original.cursor, &release, None).unwrap();
    let source_manifest = std::fs::read(release.join("release-manifest.json")).unwrap();
    // Different live project must not be cloned or confused with historical provenance.
    engine
        .new_project(
            viamind_engine::NewProject { name: "Unrelated live board".into(), expected_cursor: Some(original.cursor), discard_dirty: true },
            viamind_engine::Origin::User,
        )
        .unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let server =
        viamind_mcp::serve_http(viamind_mcp::engine_registry(engine.clone()).unwrap().read_only(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "release-reader-a").await;
    let b = connect(server.url(), "release-reader-b").await;
    let tools = a.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "inspect_output_release"));
    assert!(!tools.iter().any(|t| t.name == "run_output_job"));
    let expected =
        serde_json::to_value(viamind_fab::inspect_output_release(&viamind_fab::InspectOutputRelease { path: release.clone() }).unwrap()).unwrap();
    for client in [&a, &b] {
        let result = call(client, "inspect_output_release", json!({"path":release})).await;
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.structured_content.unwrap(), expected);
    }
    assert_eq!(expected["exported_cursor"], json!(original.cursor));
    assert_ne!(expected["project_id"], json!(before.project.id));
    assert_eq!(expected["matched_assembly"]["component_count"], 2);
    for args in
        [json!({"path":release,"extra":true}), json!({"path":"x".repeat(4097)}), json!({"path":native}), json!({"path":temp.path().join("missing")})]
    {
        assert_eq!(call(&b, "inspect_output_release", args).await.is_error, Some(true));
    }
    let file = release.join("assembly/populated-bom.csv");
    let old = std::fs::read(&file).unwrap();
    std::fs::write(&file, b"changed").unwrap();
    assert_eq!(call(&a, "inspect_output_release", json!({"path":release})).await.is_error, Some(true));
    std::fs::write(file, old).unwrap();
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read(&native).unwrap(), native_bytes);
    assert_eq!(std::fs::read(release.join("release-manifest.json")).unwrap(), source_manifest);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
