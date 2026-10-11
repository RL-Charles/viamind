//! Actual HTTP DRC clients share scoped immutable snapshot results with the desktop engine.
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{engine_registry, serve_http};
#[tokio::test(flavor = "multi_thread")]
async fn batch_drc_is_discoverable_read_only_shared_bounded_and_reflects_new_edits() {
    let engine = EngineHandle::new(Project::new("DRC HTTP"));
    for name in ["A", "B", "C", "Legacy"] {
        engine.execute("add_part", json!({"designator":name,"value":"test","footprint":"explicit","x_mm":0,"y_mm":0}), Origin::User).unwrap();
        if name != "Legacy" {
            let part = engine.read(|p| p.board.part_by_designator(name).unwrap().clone());
            engine
                .execute(
                    "set_part_pads",
                    json!({"id":part.id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":name}]}),
                    Origin::User,
                )
                .unwrap();
        }
    }
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("drc-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("drc-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let tools = a.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|t| t.name == "run_drc").unwrap();
    assert_eq!(serde_json::to_value(&tool.input_schema).unwrap(), viamind_engine::input_schema::<viamind_drc::DrcInput>());
    let before = engine.session_state();
    let result = a.call_tool(CallToolRequestParams::new("run_drc").with_arguments(Default::default())).await.unwrap();
    assert_eq!(result.is_error, Some(false));
    let report = result.structured_content.unwrap();
    assert_eq!(report["source_cursor"], serde_json::to_value(before.cursor).unwrap());
    assert_eq!(report["report"]["violations"].as_array().unwrap().len(), 3);
    assert_eq!(report["report"]["coverage"]["excluded_parts"].as_array().unwrap().len(), 1);
    assert_eq!(engine.session_state(), before);
    let other = b.call_tool(CallToolRequestParams::new("run_drc").with_arguments(Default::default())).await.unwrap().structured_content.unwrap();
    assert_eq!(other, report);
    let limited = b
        .call_tool(CallToolRequestParams::new("run_drc").with_arguments(json!({"max_violations":1}).as_object().unwrap().clone()))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(limited["report"]["truncated"], true);
    for bad in [
        json!({"copper_clearance_mm":-1}),
        json!({"copper_clearance_mm":null}),
        json!({"edge_clearance_mm":101}),
        json!({"max_violations":0}),
        json!({"unknown":true}),
        json!({"minimum_via_drill_mm":null}),
        json!({"minimum_annular_width_mm":-1}),
        json!({"hole_clearance_mm":101}),
        json!({"hole_to_copper_clearance_mm":-1}),
    ] {
        assert_eq!(
            a.call_tool(CallToolRequestParams::new("run_drc").with_arguments(bad.as_object().unwrap().clone())).await.unwrap().is_error,
            Some(true)
        );
        assert_eq!(engine.session_state(), before);
    }
    let id = engine.read(|p| p.board.part_by_designator("C").unwrap().id);
    engine.execute("move_part", json!({"id":id,"x_mm":10,"y_mm":0}), Origin::User).unwrap();
    let refreshed = b.call_tool(CallToolRequestParams::new("run_drc").with_arguments(Default::default())).await.unwrap().structured_content.unwrap();
    assert_ne!(refreshed["source_cursor"], report["source_cursor"]);
    assert_eq!(refreshed["report"]["violations"].as_array().unwrap().len(), 1);
    let reference = &refreshed["report"]["violations"][0]["objects"][0];
    assert!(reference["part_id"].is_string());
    assert!(reference["pad_id"].is_string());
    assert_ne!(refreshed["report"]["coverage"]["checks_not_performed"], Value::Null);
    a.call_tool(
        CallToolRequestParams::new("add_track").with_arguments(
            json!({"start_x_mm":100,"start_y_mm":100,"end_x_mm":101,"end_y_mm":100,"width_mm":0.2,"layer":"bottom","net":"track-net"})
                .as_object()
                .unwrap()
                .clone(),
        ),
    )
    .await
    .unwrap();
    b.call_tool(
        CallToolRequestParams::new("add_via")
            .with_arguments(json!({"x_mm":100,"y_mm":100,"diameter_mm":0.6,"drill_mm":0.3,"net":"via-net"}).as_object().unwrap().clone()),
    )
    .await
    .unwrap();
    let mixed_before = engine.session_state();
    let input = json!({"minimum_via_drill_mm":0.4});
    let mixed = a
        .call_tool(CallToolRequestParams::new("run_drc").with_arguments(input.as_object().unwrap().clone()))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    let mixed_other = b
        .call_tool(CallToolRequestParams::new("run_drc").with_arguments(input.as_object().unwrap().clone()))
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(mixed, mixed_other);
    assert_eq!(engine.session_state(), mixed_before);
    assert_eq!(mixed["report"]["coverage"]["checked_tracks"], 1);
    assert_eq!(mixed["report"]["coverage"]["checked_vias"], 1);
    let issues = mixed["report"]["violations"].as_array().unwrap();
    assert!(issues.iter().any(|v| v["kind"] == "minimum_via_drill" && v["objects"][0]["kind"] == "via" && v["objects"][0]["via_id"].is_string()));
    assert!(issues.iter().any(|v| v["kind"] == "copper_clearance"
        && v["objects"].as_array().unwrap().iter().any(|o| o["kind"] == "track" && o["track_id"].is_string())
        && v["objects"].as_array().unwrap().iter().any(|o| o["kind"] == "via")));
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
