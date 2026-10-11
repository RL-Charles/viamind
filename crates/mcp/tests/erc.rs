//! Two actual MCP clients use transient pin policy over one shared exact snapshot.
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::Project;
use viamind_engine::{EngineHandle, ErcInput};
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
fn symbol(name: &str, x: i64, kind: &str) -> Value {
    json!({"kind":"symbol","symbol":{"designator":name,"value":"","position":{"x":x,"y":0},"rotation_quarters":0,"mirrored":false,"width":10,"height":10,"pins":[{"number":"1","name":"","position":{"x":0,"y":0},"electrical":kind}]}})
}
#[tokio::test(flavor = "multi_thread")]
async fn live_clients_share_exact_erc_with_configurable_matrix_guards_undo_and_honest_coverage() {
    let engine = EngineHandle::new(Project::new("ERC HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("erc-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("erc-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|t| t.name == "run_erc").unwrap();
    assert_eq!(serde_json::to_value(&tool.input_schema).unwrap(), viamind_engine::input_schema::<ErcInput>());
    let project = engine.read(|p| p.id);
    let edits = json!([symbol("U1",0,"output"),symbol("U2",0,"output"),symbol("U3",100,"input"),symbol("U4",200,"power_input"),{"kind":"label","position":{"x":200,"y":0},"name":"VCC","scope":"power"}]);
    assert_eq!(call(&a, "edit_schematic_sheet", json!({"expected_project":project,"name":"S","edits":edits})).await.is_error, Some(false));
    let before = engine.session_state();
    let dirty = engine.document_status();
    let undo = engine.next_undo();
    let r = call(&a, "run_erc", json!({})).await.structured_content.unwrap();
    assert_eq!(r["violations"].as_array().unwrap().len(), 4);
    assert_eq!(r["complete"], true);
    assert_eq!(r["coverage"]["full_erc"], false);
    assert_eq!(r["source_cursor"], serde_json::to_value(before.cursor).unwrap());
    assert_eq!(r["effective_matrix"].as_array().unwrap().len(), 66);
    for issue in r["violations"].as_array().unwrap() {
        assert!(issue["id"].is_string());
        assert!(issue["net_id"].is_string());
        for pin in issue["pins"].as_array().unwrap() {
            assert!(pin["sheet_id"].is_string());
            assert!(pin["symbol_id"].is_string());
            assert!(pin["id"].is_string());
            assert!(pin["electrical"].is_string());
        }
    }
    assert_eq!(call(&b, "run_erc", json!({})).await.structured_content.unwrap(), r);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), dirty);
    assert_eq!(engine.next_undo(), undo);
    let limited = call(&b, "run_erc", json!({"max_violations":1})).await.structured_content.unwrap();
    assert_eq!(limited["complete"], false);
    assert_eq!(limited["stop_reason"], "violation_limit");
    assert_eq!(limited["violations"], json!([r["violations"][0].clone()]));
    let config = json!({"compatibility":[{"a":"output","b":"output","severity":"allow"}],"unconnected_input":"allow","undriven_input":"warning","missing_power":"allow"});
    let configured = call(&b, "run_erc", config).await.structured_content.unwrap();
    assert_eq!(configured["violations"].as_array().unwrap().len(), 1);
    assert_eq!(configured["violations"][0]["severity"], "warning");
    assert_eq!(call(&a, "run_erc", json!({})).await.structured_content.unwrap(), r, "policy is per request, not a hidden shared mutation");
    for bad in [
        json!({"max_violations":0}),
        json!({"max_violations":10001}),
        json!({"missing_power":"fatal"}),
        json!({"unknown":true}),
        json!({"compatibility":[{"a":"input","b":"output","severity":"allow"},{"a":"output","b":"input","severity":"error"}]}),
    ] {
        assert_eq!(call(&b, "run_erc", bad).await.is_error, Some(true));
        assert_eq!(engine.session_state(), before);
    }
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(true), "read-only policy checks do not create undoable work");
    let sheet = before.project.schematic.sheets.values().next().unwrap();
    assert_eq!(call(&a,"edit_schematic_sheet",json!({"expected_project":project,"sheet_id":sheet.id,"expected":sheet,"name":"S","edits":[{"kind":"wire","start":{"x":0,"y":0},"end":{"x":100,"y":0}}]})).await.is_error,Some(false));
    let changed = call(&b, "run_erc", json!({})).await.structured_content.unwrap();
    assert_eq!(changed["violations"].as_array().unwrap().len(), 2);
    assert_ne!(changed["source_cursor"], r["source_cursor"]);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    let undone = call(&b, "run_erc", json!({})).await.structured_content.unwrap();
    assert_eq!(undone["violations"], r["violations"]);
    assert_ne!(undone["source_cursor"], r["source_cursor"]);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
