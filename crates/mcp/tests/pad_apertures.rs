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
async fn real_two_clients_share_exact_policy_guards_library3_and_independent_reuse() {
    let dir = tempfile::tempdir().unwrap();
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap());
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("apertures-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("apertures-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "set_pad_apertures"));
    let before = engine.session_state();
    let part = before.project.board.parts.values().next().unwrap();
    let pad = &part.pads.as_ref().unwrap().0[0];
    let edit = json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"expected":part,"pad_id":pad.id,
        "apertures":{"mask":{"width":3,"height":5},"paste":null}});
    assert_eq!(call(&a, "set_pad_apertures", edit.clone()).await.is_error, Some(false));
    let changed = engine.session_state();
    let mut expected = before.project.clone();
    expected.board.parts.get_mut(&part.id).unwrap().pads.as_mut().unwrap().0[0].apertures =
        Some(serde_json::from_value(json!({"mask":{"width":3,"height":5},"paste":null})).unwrap());
    assert_eq!(changed.project, expected);
    assert_eq!(call(&b, "get_design", json!({})).await.structured_content.unwrap(), serde_json::to_value(&expected).unwrap());
    assert_eq!(call(&b, "set_pad_apertures", edit.clone()).await.is_error, Some(true));
    assert_eq!(engine.session_state(), changed);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), before.project);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), expected);
    let current = engine.session_state();
    let status = engine.document_status();
    let mut f = viamind_doc::footprint::Footprint::from_part(&current.project.board.parts[&part.id]).unwrap();
    f.pads[0].width.0 += 1;
    let path = dir.path().join("openings.json");
    assert_eq!(call(&b, "save_footprint_definition", json!({"path":path,"footprint":f})).await.is_error, Some(false));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["version"], 3);
    assert_eq!(call(&a, "load_footprint_definition", json!({"path":path})).await.structured_content.unwrap(), serde_json::to_value(&f).unwrap());
    assert_eq!(engine.session_state(), current);
    assert_eq!(engine.document_status(), status);
    let mut reuse = json!({"expected_project":current.project.id,"expected":current.project.board.parts[&part.id],"path":path});
    assert_eq!(call(&a, "import_footprint", reuse.clone()).await.is_error, Some(true));
    assert_eq!(engine.session_state(), current);
    reuse["expected_cursor"] = serde_json::to_value(current.cursor).unwrap();
    assert_eq!(call(&a, "import_footprint", reuse).await.is_error, Some(false));
    let after = engine.read(|p| p.board.parts[&part.id].clone());
    assert_eq!(after.pads.as_ref().unwrap().0[0].id, pad.id);
    assert_eq!(after.pads.as_ref().unwrap().0[0].net, pad.net);
    assert_eq!(after.pads.as_ref().unwrap().0[0].apertures, f.pads[0].apertures);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), current.project);
    let checkpoint = engine.session_state();
    let mut invalid = edit;
    invalid["expected"]["pads"] = json!(vec![Value::Null; 1025]);
    assert_eq!(call(&b, "set_pad_apertures", invalid).await.is_error, Some(true));
    assert_eq!(engine.session_state(), checkpoint);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
