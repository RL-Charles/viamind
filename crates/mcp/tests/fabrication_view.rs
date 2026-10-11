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
async fn call(client: &Client, name: &'static str, input: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn two_real_clients_inspect_exact_files_without_mutating_and_share_strict_cursor_guards() {
    let e = EngineHandle::new(viamind_doc::Project::new("keep"));
    let before = e.session_state();
    let status = e.document_status();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("top.gbr");
    std::fs::write(&path,format!("%FSLAX56Y56*%%MOMM*%%TF.FileFunction,Copper,L1,Top*%%TF.FilePolarity,Positive*%%TF.SameCoordinates,{}*%%ADD10R,2.000001X1.000003*%%LPD*%%LMN*%%LS1*%G01*D10*%LR90.001*%X-1234567Y2000003D03*M02*",before.project.id)).unwrap();
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("overlay-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("overlay-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    assert!(a.list_all_tools().await.unwrap().iter().any(|t| t.name == "inspect_fabrication_files"));
    let request = json!({"paths":[path],"expected_project":before.project.id,"expected_cursor":before.cursor});
    let first = call(&a, "inspect_fabrication_files", request.clone()).await.structured_content.unwrap();
    let second = call(&b, "inspect_fabrication_files", request.clone()).await.structured_content.unwrap();
    assert_eq!(first, second);
    assert_eq!(first["files"][0]["geometry"]["primitives"][0]["center"], json!({"x":-1234567,"y":-2000003}));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    let mut extra = request.clone();
    extra["extra"] = json!(true);
    assert_eq!(call(&a, "inspect_fabrication_files", extra).await.is_error, Some(true));
    let mut wrong = request.clone();
    wrong["expected_project"] = json!(viamind_doc::ObjectId::new());
    assert_eq!(call(&a, "inspect_fabrication_files", wrong).await.is_error, Some(true));
    call(&b, "add_part", json!({"designator":"PEER","value":"keep","footprint":"f","x_mm":0,"y_mm":0})).await;
    let peer = e.session_state();
    assert_eq!(call(&a, "inspect_fabrication_files", request).await.is_error, Some(true));
    assert_eq!(e.session_state(), peer);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
