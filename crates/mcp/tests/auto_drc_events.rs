use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::EngineHandle;
use viamind_mcp::{engine_registry, serve_http};

type Client = RunningService<RoleClient, ClientConfig>;
async fn connect(url: String) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new("DRC observer", "1"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(client: &Client, name: &str, args: Value) -> rmcp::model::CallToolResult {
    client.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
async fn value(client: &Client, name: &str, args: Value) -> Value {
    let result = call(client, name, args).await;
    assert_eq!(result.is_error, Some(false), "{result:?}");
    result.structured_content.unwrap()
}
async fn terminal(client: &Client, mut cursor: Value) -> Value {
    for _ in 0..10 {
        let batch = value(client, "watch_auto_drc_events", json!({"after":cursor,"timeout_ms":1000})).await;
        cursor = batch["cursor"].clone();
        if let Some(event) =
            batch["events"].as_array().unwrap().iter().find(|e| ["ready", "skipped", "failed"].contains(&e["phase"].as_str().unwrap()))
        {
            return event.clone();
        }
    }
    panic!("automatic DRC terminal event timeout");
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_http_observers_resume_page_and_follow_peer_undo_and_same_uuid_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().join("protected.json");
    let project = viamind_doc::Project::new("DRC events");
    viamind_doc::file::save(&project, &native).unwrap();
    let bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url()).await;
    let b = connect(server.url()).await;
    let tools = a.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "get_auto_drc_event_state"));
    assert_eq!(
        serde_json::to_value(&tools.iter().find(|t| t.name == "watch_auto_drc_events").unwrap().input_schema).unwrap(),
        viamind_engine::input_schema::<viamind_engine::WatchAutoDrcEvents>()
    );
    let baseline = value(&a, "get_auto_drc_event_state", json!({})).await;
    assert_eq!(baseline["cursor"]["sequence"], 0);
    assert_ne!(baseline["cursor"]["stream_id"], json!(before.cursor.stream_id));
    let waiting = value(&a, "watch_auto_drc_events", json!({"after":baseline["cursor"],"timeout_ms":5000,"limit":1}));
    let configuring = value(&b, "configure_auto_drc", json!({"enabled":true,"debounce_ms":0}));
    let (first, configured) = tokio::join!(waiting, configuring);
    assert_eq!(configured["generation"], 1);
    assert_eq!(first["events"][0]["phase"], "pending");
    let ready = terminal(&b, baseline["cursor"].clone()).await;
    assert_eq!(ready["generation"], 1);
    assert_eq!(ready["report"]["violation_count"], 0);
    assert_eq!(ready["source_cursor"], json!(before.cursor));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    a.cancel().await.unwrap();
    let a = connect(server.url()).await;
    let rest = value(&a, "watch_auto_drc_events", json!({"after":first["cursor"],"limit":1})).await;
    assert_eq!(rest["events"][0]["phase"], "running");
    assert_eq!(rest["has_more"], true);
    let final_page = value(&a, "watch_auto_drc_events", json!({"after":rest["cursor"]})).await;
    assert_eq!(final_page["events"][0], ready);
    let baseline = value(&a, "get_auto_drc_event_state", json!({})).await;
    assert_eq!(baseline["cursor"], final_page["cursor"]);
    assert_eq!(baseline["status"]["generation"], ready["generation"]);
    assert_eq!(baseline["status"]["source_cursor"], ready["source_cursor"]);
    value(&b, "add_via", json!({"x_mm":10,"y_mm":10,"diameter_mm":0.6,"drill_mm":0.1,"net":"A"})).await;
    let changed = engine.session_state();
    let ready = terminal(&a, baseline["cursor"].clone()).await;
    assert_eq!(ready["generation"], 2);
    assert_eq!(ready["source_cursor"], json!(changed.cursor));
    assert_eq!(ready["report"]["violation_count"], 1);
    let baseline = value(&a, "get_auto_drc_event_state", json!({})).await;
    value(&b, "undo", json!({})).await;
    let ready = terminal(&a, baseline["cursor"].clone()).await;
    assert_eq!(ready["generation"], 3);
    assert_eq!(engine.session_state().project, before.project);
    let baseline = value(&a, "get_auto_drc_event_state", json!({})).await;
    let old = engine.event_cursor();
    value(&b, "open_project", json!({"path":native,"expected_cursor":old,"discard_dirty":true})).await;
    let ready = terminal(&a, baseline["cursor"].clone()).await;
    assert_eq!(ready["generation"], 4);
    assert_eq!(engine.session_state().project.id, before.project.id);
    assert_ne!(ready["source_cursor"]["stream_id"], json!(old.stream_id));
    assert_eq!(value(&a, "get_auto_drc_event_state", json!({})).await["cursor"]["stream_id"], baseline["cursor"]["stream_id"]);
    let stable = engine.session_state();
    for invalid in [
        json!({"limit":0}),
        json!({"limit":129}),
        json!({"timeout_ms":30001}),
        json!({"extra":true}),
        json!({"after":{"stream_id":old.stream_id,"sequence":0}}),
    ] {
        assert_eq!(call(&a, "watch_auto_drc_events", invalid).await.is_error, Some(true));
    }
    assert_eq!(call(&a, "get_auto_drc_event_state", json!({"extra":true})).await.is_error, Some(true));
    assert_eq!(engine.session_state(), stable);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_read_only_observers_report_exact_retention_and_refuse_configuration() {
    let engine = EngineHandle::new(viamind_doc::Project::new("read only DRC events"));
    let baseline = engine.auto_drc_event_state();
    for _ in 0..129 {
        engine.configure_auto_drc(Default::default()).unwrap();
    }
    let before = engine.session_state();
    let state = engine.auto_drc_event_state();
    let server = serve_http(engine_registry(engine.clone()).unwrap().read_only(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url()).await;
    let b = connect(server.url()).await;
    for client in [&a, &b] {
        let tools = client.list_all_tools().await.unwrap();
        assert!(tools.iter().any(|t| t.name == "get_auto_drc_event_state"));
        assert!(tools.iter().any(|t| t.name == "watch_auto_drc_events"));
        assert!(!tools.iter().any(|t| t.name == "configure_auto_drc"));
        assert!(
            client
                .call_tool(CallToolRequestParams::new("configure_auto_drc").with_arguments(json!({"enabled":true}).as_object().unwrap().clone()))
                .await
                .is_err()
        );
        assert_eq!(call(client, "watch_auto_drc_events", json!({"after":baseline.cursor})).await.is_error, Some(true));
        let floor = viamind_engine::AutoDrcEventCursor { sequence: 1, ..baseline.cursor };
        let page = value(client, "watch_auto_drc_events", json!({"after":floor,"limit":128})).await;
        assert_eq!(page["events"].as_array().unwrap().len(), 128);
        assert_eq!(page["events"][0]["sequence"], 2);
        assert_eq!(page["events"][127]["sequence"], 129);
        assert_eq!(page["has_more"], false);
        let empty = value(client, "watch_auto_drc_events", json!({"after":state.cursor,"timeout_ms":1})).await;
        assert_eq!(empty["events"], json!([]));
        assert_eq!(empty["cursor"], json!(state.cursor));
        assert_eq!(
            call(client, "watch_auto_drc_events", json!({"after":{"stream_id":state.cursor.stream_id,"sequence":130}})).await.is_error,
            Some(true)
        );
    }
    assert_eq!(engine.auto_drc_event_state(), state);
    assert_eq!(engine.session_state(), before);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
