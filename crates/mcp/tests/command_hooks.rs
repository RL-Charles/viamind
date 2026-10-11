use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::Project;
use viamind_engine::{
    EngineHandle, Origin,
    agent_policy::{AgentMode, AgentPolicyError},
};
use viamind_mcp::{engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
async fn connect(url: String) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new("hook observer", "1"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(c: &Client, name: &str, args: Value) -> rmcp::model::CallToolResult {
    c.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
async fn value(c: &Client, name: &str, args: Value) -> Value {
    let r = call(c, name, args).await;
    assert_eq!(r.is_error, Some(false), "{r:?}");
    r.structured_content.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn actual_two_client_gate_rollback_repair_undo_redo_and_all_mode_admission() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("protected.json");
    viamind_doc::file::save(&Project::new("hooks"), &native).unwrap();
    let bytes = std::fs::read(&native).unwrap();
    let e = EngineHandle::load(&native).unwrap();
    let before = e.session_state();
    let status = e.document_status();
    let auto = e.auto_drc_status();
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url()).await;
    let b = connect(server.url()).await;
    let tools = a.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "get_command_hooks"));
    assert!(tools.iter().any(|t| t.name == "configure_command_hooks"));
    value(&a, "configure_command_hooks", json!({"enabled":true})).await;
    assert_eq!(value(&a, "get_command_hooks", json!({})).await, value(&b, "get_command_hooks", json!({})).await);
    let add = json!({"designator":"R1","value":"legacy","footprint":"F","x_mm":0,"y_mm":0});
    assert_eq!(call(&b, "add_part", add.clone()).await.is_error, Some(true));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    assert_eq!(e.auto_drc_status(), auto);
    value(&a, "configure_command_hooks", json!({"enabled":false})).await;
    let added = value(&b, "add_part", add).await;
    value(&a, "configure_command_hooks", json!({"enabled":true})).await;
    value(&b, "undo", json!({})).await;
    let undone = e.session_state();
    assert_eq!(call(&b, "redo", json!({})).await.is_error, Some(true));
    assert_eq!(e.session_state(), undone);
    assert_eq!(e.with_engine(|e| e.next_redo().unwrap().id.to_string()), format!("tx{}", added["tx"]));
    for mode in [AgentMode::Approval, AgentMode::ReadOnly] {
        e.agent_policy().set_mode(mode).unwrap();
        let tools = a.list_all_tools().await.unwrap();
        assert!(tools.iter().any(|t| t.name == "get_command_hooks"));
        assert!(!tools.iter().any(|t| t.name == "configure_command_hooks"));
        assert_eq!(call(&a, "configure_command_hooks", json!({"enabled":false})).await.is_error, Some(true));
        value(&b, "get_command_hooks", json!({})).await;
    }
    e.agent_policy().lock_read_only().unwrap();
    assert_eq!(call(&b, "configure_command_hooks", json!({"enabled":false})).await.is_error, Some(true));
    assert_eq!(e.session_state(), undone);
    assert_eq!(std::fs::read(native).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
#[tokio::test(flavor = "multi_thread")]
async fn abandoned_transport_keeps_writer_permit_until_post_callback_finishes() {
    let e = EngineHandle::new(Project::new("retained post"));
    let entered = std::sync::Arc::new(tokio::sync::Notify::new());
    let notify = entered.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let wait = std::sync::Mutex::new(wait);
    e.register_post_command_hook(
        "hold".into(),
        std::sync::Arc::new(move |_| {
            notify.notify_one();
            wait.lock().unwrap().recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            Ok(())
        }),
    )
    .unwrap();
    let server = serve_http(engine_registry(e.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = std::sync::Arc::new(connect(server.url()).await);
    let client = a.clone();
    let writing =
        tokio::spawn(async move { value(&client, "add_part", json!({"designator":"R1","value":"","footprint":"F","x_mm":0,"y_mm":0})).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified()).await.unwrap();
    assert_eq!(e.agent_policy().set_mode(AgentMode::ReadOnly), Err(AgentPolicyError::Busy(1)));
    writing.abort();
    assert_eq!(e.agent_policy().set_mode(AgentMode::Approval), Err(AgentPolicyError::Busy(1)));
    release.send(()).unwrap();
    for _ in 0..100 {
        if e.agent_policy().status().active_writers == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(e.agent_policy().status().active_writers, 0);
    e.agent_policy().set_mode(AgentMode::Approval).unwrap();
    assert_eq!(e.read(|p| p.board.parts.len()), 1);
    e.undo(Origin::User).unwrap();
    server.shutdown();
}
