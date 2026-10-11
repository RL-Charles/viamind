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

    // Selection is shared with the UI, transient, and attributed to this agent.
    let id = engine.read(|p| *p.board.parts.keys().next().unwrap());
    let selected = call(&agent, "set_selection", json!({ "ids": [id] })).await;
    assert_eq!(selected.is_error, Some(false), "{selected:?}");
    assert_eq!(engine.selection().ids, [id]);
    assert!(matches!(engine.selection().origin, Some(Origin::Agent { .. })));
    let selection = call(&agent, "get_selection", json!({})).await.structured_content.unwrap();
    assert_eq!(selection["ids"], json!([id]));
    let invalid = call(&agent, "set_selection", json!({ "ids": [viamind_doc::ObjectId::new()] })).await;
    assert_eq!(invalid.is_error, Some(true));
    assert_eq!(engine.selection().ids, [id]);
    assert_eq!(call(&agent, "get_selection", json!({ "unexpected": true })).await.is_error, Some(true));

    // Bad input comes back as a tool error the agent can read, and changes nothing.
    let dup = call(&agent, "add_part", resistor("R1", 30.0)).await;
    assert_eq!(dup.is_error, Some(true));
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);

    // The agent undoes its own change, then redoes it.
    assert_eq!(call(&agent, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(|p| p.board.parts.len()), 0);
    assert!(engine.selection().ids.is_empty());
    assert_eq!(call(&agent, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);

    // The human undoes the agent's edit from the UI.
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.parts.len()), 0);

    agent.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_query_the_same_live_parts_with_filters_pages_and_useful_errors() {
    let engine = EngineHandle::new(Project::new("query HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let writer = connect(server.url(), "query-writer").await;
    let reader = connect(server.url(), "query-reader").await;
    let tools = reader.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|tool| tool.name == "query_parts").expect("query_parts discoverable");
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    assert_eq!(schema, viamind_engine::input_schema::<viamind_engine::PartQuery>());
    assert_eq!(schema["properties"]["limit"]["minimum"], 1);
    assert_eq!(schema["properties"]["limit"]["maximum"], 1000);
    assert_eq!(schema["additionalProperties"], false);

    let mut ids = Vec::new();
    for (designator, x) in [("R1", 10.0), ("R2", 20.0), ("R3", 30.0)] {
        let added = call(&writer, "add_part", resistor(designator, x)).await;
        ids.push(added.structured_content.unwrap()["created"][0].clone());
    }
    ids.sort_by_key(Value::to_string);
    let all = call(&reader, "query_parts", json!({ "text": "10K", "limit": 2 })).await;
    assert_eq!(all.is_error, Some(false));
    let page = all.structured_content.unwrap();
    assert_eq!(page["total"], 3);
    let page_ids: Vec<_> = page["parts"].as_array().unwrap().iter().map(|part| part["id"].clone()).collect();
    assert_eq!(page_ids, ids[..2]);
    assert_eq!(page["next_after"], ids[1]);
    let last = call(&reader, "query_parts", json!({ "text": "10K", "limit": 2, "after": page["next_after"] })).await.structured_content.unwrap();
    assert_eq!(last["parts"][0]["id"], ids[2]);
    assert_eq!(last["total"], 3);
    assert_eq!(last["next_after"], Value::Null);
    let filtered = call(
        &reader,
        "query_parts",
        json!({
            "designator": "R2", "footprint": "R_0603", "side": "top",
            "bounds": { "min_x_mm": 20, "min_y_mm": 20, "max_x_mm": 20, "max_y_mm": 20 }
        }),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["parts"][0]["position"], json!({ "x": 20_000_000, "y": 20_000_000 }));
    let r2 = filtered["parts"][0]["id"].clone();
    assert_eq!(call(&writer, "move_part", json!({ "id": r2, "x_mm": 21, "y_mm": 22 })).await.is_error, Some(false));
    let moved = call(&reader, "query_parts", json!({ "ids": [r2] })).await.structured_content.unwrap();
    assert_eq!(moved["parts"][0]["position"], json!({ "x": 21_000_000, "y": 22_000_000 }));
    let before = engine.read(Clone::clone);
    for (input, field) in [
        (json!({ "limit": 1001 }), "limit"),
        (json!({ "bogus": 1 }), "bogus"),
        (json!({ "side": "front" }), "front"),
        (json!({ "ids": ["bad-uuid"] }), "UUID"),
        (json!({ "bounds": { "min_x_mm": 2, "min_y_mm": 0, "max_x_mm": 1, "max_y_mm": 1 } }), "bounds"),
    ] {
        let error = call(&reader, "query_parts", input).await;
        assert_eq!(error.is_error, Some(true), "{error:?}");
        assert!(format!("{:?}", error.content).to_lowercase().contains(&field.to_lowercase()), "{error:?}");
    }
    assert_eq!(engine.read(Clone::clone), before);
    // Reads never become this reader's undoable edits.
    assert_eq!(call(&reader, "undo", json!({})).await.is_error, Some(true));
    reader.cancel().await.unwrap();
    writer.cancel().await.unwrap();
    server.shutdown();
}
