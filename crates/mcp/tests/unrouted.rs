use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::{EngineHandle, UnroutedNetQuery, agent_policy::AgentMode};
use viamind_mcp::{agent::CallContext, engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn call(c: &Client, args: Value) -> rmcp::model::CallToolResult {
    c.call_tool(CallToolRequestParams::new("query_unrouted_nets").with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "multi_thread")]
async fn actual_same_name_two_client_whole_inventory_in_every_mode_matches_typed_result_and_preserves_native_session() {
    let _serial = SERIAL.lock().await;
    for mode in [AgentMode::Live, AgentMode::Approval, AgentMode::ReadOnly] {
        let project = viamind_doc::file::from_str(include_str!("../../../fixtures/connectivity/one-call.viamind.json")).unwrap();
        let engine = EngineHandle::new(project);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("protected.viamind.json");
        engine.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
        let native = std::fs::read(&path).unwrap();
        if mode == AgentMode::ReadOnly {
            engine.agent_policy().lock_read_only().unwrap();
        } else {
            engine.agent_policy().set_mode(mode).unwrap();
        }
        let expected = engine.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap().run_value().unwrap();
        let before = engine.session_state();
        let status = engine.document_status();
        let drc = engine.auto_drc_event_state();
        let events = engine.subscribe();
        let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
        let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("same observer", "1"))
            .serve(StreamableHttpClientTransport::from_uri(server.url()))
            .await
            .unwrap();
        let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("same observer", "1"))
            .serve(StreamableHttpClientTransport::from_uri(server.url()))
            .await
            .unwrap();
        let tools = a.list_all_tools().await.unwrap();
        let tool = tools.iter().find(|t| t.name == "query_unrouted_nets").unwrap();
        assert_eq!(serde_json::to_value(&tool.input_schema).unwrap(), viamind_engine::input_schema::<UnroutedNetQuery>());
        for c in [&a, &b] {
            let r = call(c, json!({})).await;
            assert_eq!(r.is_error, Some(false));
            assert_eq!(r.structured_content.unwrap(), expected);
            assert_eq!(call(c, json!({"limit":1})).await.is_error, Some(true));
        }
        assert_eq!(expected["nets"].as_array().unwrap().len(), 7);
        assert_eq!(expected["connected_count"], 2);
        assert_eq!(expected["unrouted_count"], 1);
        assert_eq!(expected["unknown_count"], 1);
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
        assert_eq!(engine.auto_drc_event_state(), drc);
        assert_eq!(std::fs::read(path).unwrap(), native);
        assert!(events.try_recv().is_err());
        a.cancel().await.unwrap();
        b.cancel().await.unwrap();
        server.shutdown();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_actual_registry_awaiter_keeps_queued_capture_admission_until_worker_retires() {
    let _serial = SERIAL.lock().await;
    let engine = EngineHandle::new(viamind_doc::Project::new("Blocked actual query"));
    let other = engine.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap();
    let registry = engine_registry(engine.clone()).unwrap();
    let (locked, ready) = std::sync::mpsc::sync_channel(1);
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let held = engine.clone();
    let lock_thread = std::thread::spawn(move || {
        held.with_engine(|_| {
            locked.send(()).unwrap();
            wait.recv().unwrap();
        })
    });
    ready.recv().unwrap();
    let pending = tokio::spawn(async move { registry.call("query_unrouted_nets", json!({}), CallContext::new("cancelled observer")).await });
    let mut admitted = false;
    for _ in 0..200 {
        match engine.prepare_unrouted_nets(UnroutedNetQuery {}) {
            Ok(probe) => drop(probe),
            Err(_) => {
                admitted = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(admitted);
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(engine.prepare_unrouted_nets(UnroutedNetQuery {}).is_err());
    release.send(()).unwrap();
    lock_thread.join().unwrap();
    drop(other);
    for _ in 0..200 {
        let a = engine.prepare_unrouted_nets(UnroutedNetQuery {}).unwrap();
        if let Ok(b) = engine.prepare_unrouted_nets(UnroutedNetQuery {}) {
            drop(b);
            drop(a);
            return;
        }
        drop(a);
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("actual worker must eventually retire after capture/serialization");
}
