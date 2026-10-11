use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::{ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn call(c: &Client, n: &'static str, args: Value) -> Value {
    let r = c.call_tool(CallToolRequestParams::new(n).with_arguments(args.as_object().unwrap().clone())).await.unwrap();
    assert_ne!(r.is_error, Some(true), "{r:?}");
    r.structured_content.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn actual_independent_http_agents_see_same_conservative_guides_through_route_peer_edit_and_undo() {
    let mut project = Project::new("HTTP copper islands");
    project.board.outline = Some(
        serde_json::from_value(
            serde_json::json!({"outer":[{"x":0,"y":0},{"x":100000000,"y":0},{"x":100000000,"y":100000000},{"x":0,"y":100000000}],"cutouts":[]}),
        )
        .unwrap(),
    );
    for (i, x) in [10_000_123, 30_000_257].into_iter().enumerate() {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: serde_json::from_value(json!({"x":0,"y":0})).unwrap(),
            width: serde_json::from_value(json!(1_000_000)).unwrap(),
            height: serde_json::from_value(json!(1_000_000)).unwrap(),
            net: Some("N".into()),
        };
        let part = Part {
            artwork: None,
            assembly: None,
            id: ObjectId::new(),
            designator: format!("J{i}"),
            value: String::new(),
            footprint: String::new(),
            position: serde_json::from_value(json!({"x":x,"y":10_000_129})).unwrap(),
            rotation_mdeg: 0,
            side: Side::Bottom,
            pads: Some(Pads::try_from(vec![pad]).unwrap()),
            schematic_symbol: None,
            schematic_package: None,
        };
        project.board.parts.insert(part.id, part);
    }
    let e = EngineHandle::new(project);
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("islands-a", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("islands-b", "1"))
        .serve(StreamableHttpClientTransport::from_uri(server.url()))
        .await
        .unwrap();
    let before = e.session_state();
    let status = e.document_status();
    let initial = call(&a, "query_connectivity", json!({"net":"N"})).await;
    assert_eq!(initial["guide_count"], 1);
    assert_eq!(call(&b, "query_connectivity", json!({"net":"N"})).await, initial);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    let page = call(&a, "query_route_endpoints", json!({"net":"N"})).await;
    call(&a,"route_pad_polyline",json!({"expected_project":page["project_id"],"expected_cursor":page["source_cursor"],"start":page["endpoints"][0]["id"],"target":page["endpoints"][1]["id"],"interior":[],"grid":250000,"width":250000})).await;
    let after = e.session_state();
    let routed = call(&b, "query_connectivity", json!({"net":"N"})).await;
    assert_eq!(routed["terminal_count"], 2);
    assert_eq!(routed["guide_count"], 0);
    assert_eq!(routed["copper_proof"]["analyzed_tracks"], 1);
    assert_eq!(routed["copper_proof"]["budget_fallback"], false);
    assert_eq!(call(&a, "query_nets", json!({})).await["nets"][0]["guide_count"], 0);
    assert!(e.route_endpoint_snapshot().unwrap().guide_endpoints.is_empty());
    assert_eq!(e.session_state(), after);
    // A peer breaks the exact attachment; only its own change is undone.
    call(&b, "update_part", json!({"id":page["endpoints"][0]["id"]["part_id"],"x_mm":40})).await;
    assert_eq!(call(&a, "query_connectivity", json!({"net":"N"})).await["guide_count"], 1);
    call(&b, "undo", json!({})).await;
    assert_eq!(e.read(Clone::clone), after.project);
    assert_eq!(call(&a, "query_connectivity", json!({"net":"N"})).await["guide_count"], 0);
    call(&a, "undo", json!({})).await;
    assert_eq!(e.read(Clone::clone), before.project);
    assert_eq!(call(&b, "query_connectivity", json!({"net":"N"})).await["guide_count"], 1);
    call(&a, "redo", json!({})).await;
    assert_eq!(e.read(Clone::clone), after.project);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("proof.viamind.json");
    call(&a, "save_project", json!({"path":path.to_str().unwrap()})).await;
    let loaded = viamind_doc::file::load(path).unwrap();
    assert!(viamind_engine::connection_guides(&loaded).is_empty());
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
