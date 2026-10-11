//! Real MCP transport must carry image bytes once, separately from view metadata.
use base64::Engine as _;
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    transport::StreamableHttpClientTransport,
};
use serde_json::json;
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};
#[tokio::test(flavor = "multi_thread")]
async fn schematic_png_roundtrips_as_one_real_image_with_shared_guards() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    let sheet = *project.schematic.sheets.keys().next().unwrap();
    let engine = EngineHandle::new(project);
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let agent = ClientConfig::new(ClientCapabilities::default(), Implementation::new("png-reader", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let other = ClientConfig::new(ClientCapabilities::default(), Implementation::new("png-editor", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    assert!(agent.list_all_tools().await.unwrap().iter().any(|t| t.name == "render_schematic_png"));
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transport.png");
    let result = agent
        .call_tool(
            CallToolRequestParams::new("render_schematic_png").with_arguments(
                json!({"sheet":sheet,"path":path,"expected_cursor":before.cursor,"options":{"width_px":200,"height_px":100}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    let images: Vec<_> = result.content.iter().filter_map(|c| c.as_image()).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].mime_type, "image/png");
    let bytes = base64::engine::general_purpose::STANDARD.decode(&images[0].data).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let structured = result.structured_content.unwrap();
    assert_eq!(structured["source_cursor"], json!(before.cursor));
    assert_eq!(structured["metadata"]["width_px"], 200);
    assert!(!structured.as_object().unwrap().contains_key("png_base64"));
    assert!(result.content.iter().filter_map(|c| c.as_text()).all(|t| !t.text.contains(&images[0].data)));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let edit = other
        .call_tool(
            CallToolRequestParams::new("add_via")
                .with_arguments(json!({"x_mm":4,"y_mm":5,"diameter_mm":1,"drill_mm":0.5}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(edit.is_error, Some(false));
    let stale_path = dir.path().join("stale.png");
    let stale = agent
        .call_tool(
            CallToolRequestParams::new("render_schematic_png")
                .with_arguments(json!({"sheet":sheet,"path":stale_path,"expected_cursor":before.cursor}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(stale.is_error, Some(true));
    assert!(stale.content.iter().all(|c| c.as_image().is_none()));
    assert!(!stale_path.exists());
    let undo = other.call_tool(CallToolRequestParams::new("undo")).await.unwrap();
    assert_eq!(undo.is_error, Some(false));
    assert_eq!(engine.session_state().project, before.project);
    agent.cancel().await.unwrap();
    other.cancel().await.unwrap();
    server.shutdown();
}
