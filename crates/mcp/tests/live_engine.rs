//! The P1 gate, headless: an agent connected over streamable HTTP places a resistor in the live
//! engine, the change reaches subscribers (the UI) attributed to the agent, and both the agent and
//! the human can undo it.

use std::time::Duration;

use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_doc::Project;
use viamind_engine::{ChangeKind, EngineHandle, Origin};
use viamind_mcp::{engine_registry, serve_http};

type Client = RunningService<RoleClient, ClientConfig>;

async fn connect(url: String, name: &str) -> Client {
    let transport = StreamableHttpClientTransport::from_uri(url);
    ClientConfig::new(ClientCapabilities::default(), Implementation::new(name, "1.0.0")).serve(transport).await.expect("client handshake")
}

async fn call(client: &Client, tool: &'static str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else { panic!("arguments must be an object") };
    client.call_tool(CallToolRequestParams::new(tool).with_arguments(args)).await.expect("tools/call")
}

fn resistor(designator: &str, x: f64) -> Value {
    json!({ "designator": designator, "value": "10k", "footprint": "R_0603", "x_mm": x, "y_mm": 20.0 })
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_places_resistor_live_and_it_can_be_undone() {
    let engine = EngineHandle::new(Project::new("gate"));
    let events = engine.subscribe();
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let agent = connect(server.url(), "test-agent").await;

    let tools: Vec<String> = agent.list_all_tools().await.unwrap().into_iter().map(|t| t.name.to_string()).collect();
    for expected in ["add_part", "move_part", "delete_part", "undo", "redo", "get_design"] {
        assert!(tools.iter().any(|t| t == expected), "missing tool {expected} in {tools:?}");
    }

    // The agent places R1; the engine has it immediately and subscribers hear about it.
    let result = call(&agent, "add_part", resistor("R1", 10.0)).await;
    assert_eq!(result.is_error, Some(false), "{result:?}");
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);
    let event = events.recv_timeout(Duration::from_millis(100)).expect("change event within 100 ms");
    assert_eq!(event.kind, ChangeKind::Applied);
    match &event.origin {
        Origin::Agent { id } => assert!(id.starts_with("test-agent#"), "agent id {id}"),
        Origin::User => panic!("agent edit attributed to the user"),
    }

    // get_design shows it.
    let design = call(&agent, "get_design", json!({})).await;
    let design = design.structured_content.expect("design json");
    assert_eq!(design["board"]["parts"].as_object().unwrap().len(), 1);

    // Bad input comes back as a tool error the agent can read, and changes nothing.
    let dup = call(&agent, "add_part", resistor("R1", 30.0)).await;
    assert_eq!(dup.is_error, Some(true));
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);

    // The agent undoes its own change, then redoes it.
    assert_eq!(call(&agent, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(|p| p.board.parts.len()), 0);
    assert_eq!(call(&agent, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);

    // The human undoes the agent's edit from the UI.
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.parts.len()), 0);

    agent.cancel().await.unwrap();
    server.shutdown();
}
