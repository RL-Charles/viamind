//! Real MCP HTTP clients discover and share guarded DXF imports.
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    transport::StreamableHttpClientTransport,
};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};
#[tokio::test(flavor = "multi_thread")]
async fn dxf_import_is_discoverable_and_shared_over_http() {
    let engine = EngineHandle::new(Project::new("DXF HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("dxf-editor", "1.0.0"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("dxf-observer", "1.0.0"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|tool| tool.name == "import_board_outline_dxf").unwrap();
    assert_eq!(serde_json::to_value(&tool.input_schema).unwrap(), viamind_engine::input_schema::<viamind_engine::ImportBoardOutlineDxf>());
    let before = engine.read(Clone::clone);
    let input = json!({"text":include_str!("../../doc/tests/fixtures/outline-mm.dxf"),"expected":{"project_id":before.id,"outline":null}});
    let args = input.as_object().unwrap().clone();
    let applied = a.call_tool(CallToolRequestParams::new("import_board_outline_dxf").with_arguments(args.clone())).await.unwrap();
    assert_eq!(applied.is_error, Some(false), "{applied:?}");
    let after = engine.read(Clone::clone);
    let read = b.call_tool(CallToolRequestParams::new("get_design").with_arguments(Default::default())).await.unwrap();
    assert_eq!(read.structured_content.unwrap(), serde_json::to_value(&after).unwrap());
    let stale = b.call_tool(CallToolRequestParams::new("import_board_outline_dxf").with_arguments(args)).await.unwrap();
    assert_eq!(stale.is_error, Some(true));
    assert_eq!(engine.read(Clone::clone), after);
    for (tool, expected) in [("undo", &before), ("redo", &after)] {
        assert_eq!(a.call_tool(CallToolRequestParams::new(tool).with_arguments(Default::default())).await.unwrap().is_error, Some(false));
        assert_eq!(&engine.read(Clone::clone), expected);
    }
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
