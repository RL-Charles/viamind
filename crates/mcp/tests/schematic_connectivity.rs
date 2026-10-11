//! Real HTTP clients query the same exact schematic snapshot without design changes.
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::Project;
use viamind_engine::{EngineHandle, SchematicConnectivityQuery};
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    client.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn clients_discover_exact_nets_edits_junction_ambiguity_scope_and_undo_read_only() {
    let engine = EngineHandle::new(Project::new("schematic HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("graph-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("graph-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|t| t.name == "query_schematic_connectivity").unwrap();
    assert_eq!(serde_json::to_value(&tool.input_schema).unwrap(), viamind_engine::input_schema::<SchematicConnectivityQuery>());
    let project = engine.read(|p| p.id);
    let edits = json!([
        {"kind":"wire","start":{"x":-10,"y":0},"end":{"x":10,"y":0}},
        {"kind":"wire","start":{"x":0,"y":-10},"end":{"x":0,"y":10}},
        {"kind":"label","position":{"x":-10,"y":0},"name":"VCC","scope":"global"},
        {"kind":"symbol","symbol":{"designator":"U1","value":"","position":{"x":-10,"y":-3},"rotation_quarters":1,"mirrored":true,"width":10,"height":10,"pins":[{"number":"1","name":"P","position":{"x":3,"y":0},"electrical":"power_input"}]}}
    ]);
    assert_eq!(call(&a, "edit_schematic_sheet", json!({"expected_project":project,"name":"A","edits":edits})).await.is_error, Some(false));
    let before = engine.session_state();
    let undo = engine.next_undo();
    let dirty = engine.document_status();
    let report = call(&a, "query_schematic_connectivity", json!({})).await.structured_content.unwrap();
    assert_eq!(report["total_nets"], 2);
    assert_eq!(report["source_cursor"], serde_json::to_value(before.cursor).unwrap());
    let named = report["nets"].as_array().unwrap().iter().find(|n| n["canonical_name"] == "VCC").unwrap();
    assert_eq!(named["members"].as_array().unwrap().len(), 3);
    assert!(
        named["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["kind"] == "pin" && m["symbol_id"].is_string() && m["position"] == json!({"x":-10,"y":0}))
    );
    assert_eq!(call(&b, "query_schematic_connectivity", json!({})).await.structured_content.unwrap(), report);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.next_undo(), undo);
    assert_eq!(engine.document_status(), dirty);
    for bad in [json!({"limit":0}), json!({"limit":101}), json!({"limit":null}), json!({"unknown":1}), json!({"offset":-1})] {
        assert_eq!(call(&b, "query_schematic_connectivity", bad).await.is_error, Some(true));
        assert_eq!(engine.session_state(), before);
    }
    let page = call(&b, "query_schematic_connectivity", json!({"limit":1})).await.structured_content.unwrap();
    assert_eq!(page["next_offset"], 1);
    assert_eq!(page["nets"].as_array().unwrap().len(), 1);
    let sheet = before.project.schematic.sheets.values().next().unwrap();
    assert_eq!(call(&a,"edit_schematic_sheet",json!({"expected_project":project,"sheet_id":sheet.id,"expected":sheet,"name":"A","edits":[{"kind":"label","position":{"x":0,"y":0},"name":"BAD","scope":"local"}]})).await.is_error,Some(false));
    let ambiguous = engine.session_state();
    assert_eq!(call(&b, "query_schematic_connectivity", json!({})).await.is_error, Some(true));
    assert_eq!(engine.session_state(), ambiguous);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    let after_undo = call(&b, "query_schematic_connectivity", json!({})).await.structured_content.unwrap();
    assert_eq!(after_undo["nets"], report["nets"]);
    assert_ne!(after_undo["source_cursor"], report["source_cursor"]);
    let sheet = engine.read(|p| p.schematic.sheets.values().next().unwrap().clone());
    assert_eq!(
        call(
            &a,
            "edit_schematic_sheet",
            json!({"expected_project":project,"sheet_id":sheet.id,"expected":sheet,"name":"A","edits":[{"kind":"junction","position":{"x":0,"y":0}}]})
        )
        .await
        .is_error,
        Some(false)
    );
    assert_eq!(call(&b, "query_schematic_connectivity", json!({})).await.structured_content.unwrap()["total_nets"], 1);
    assert_eq!(
        call(
            &b,
            "edit_schematic_sheet",
            json!({"expected_project":project,"name":"B","edits":[{"kind":"label","position":{"x":100,"y":0},"name":"VCC","scope":"power"}]})
        )
        .await
        .is_error,
        Some(false)
    );
    let all = call(&a, "query_schematic_connectivity", json!({})).await.structured_content.unwrap();
    assert_eq!(all["total_nets"], 1);
    assert_eq!(all["coverage"]["sheets"], 2);
    let before = engine.session_state();
    assert_eq!(call(&b, "query_schematic_connectivity", json!({})).await.structured_content.unwrap(), all);
    assert_eq!(engine.session_state(), before);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
