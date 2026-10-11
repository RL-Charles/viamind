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
async fn live_footprint_library_has_shared_identity_guards_and_connection_undo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reusable.footprint.json");
    let engine = EngineHandle::new(Project::new("Library live"));
    let id = engine.execute("add_part", resistor("R1", 10.), Origin::User).unwrap().created[0];
    let part = engine.read(|p| p.board.part(&id).unwrap().clone());
    engine
        .execute(
            "set_part_pads",
            json!({"id":id,"expected":part,"pads":[{"number":"1","x_mm":-0.800001,"y_mm":0,"width_mm":0.9,"height_mm":0.95,"net":"GND"}]}),
            Origin::User,
        )
        .unwrap();
    let snapshot = engine.session_state();
    let part = snapshot.project.board.part(&id).unwrap();
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let agent = connect(server.url(), "library-agent").await;
    let other = connect(server.url(), "other-agent").await;
    let args = json!({"expected_project":snapshot.project.id,"expected":part,"path":path});
    assert_eq!(call(&agent, "export_footprint", args.clone()).await.is_error, Some(false));
    assert_eq!(engine.session_state(), snapshot);
    assert_eq!(call(&agent, "export_footprint", args.clone()).await.is_error, Some(true));
    let mut f = viamind_doc::footprint::load(&path).unwrap();
    f.name = "Changed_library".into();
    f.pads[0].width.0 = 900003;
    std::fs::write(&path, viamind_doc::footprint::to_vec(&f).unwrap()).unwrap();
    assert_eq!(call(&agent, "import_footprint", args.clone()).await.is_error, Some(false));
    let applied = engine.read(Clone::clone);
    let changed = applied.board.part(&id).unwrap();
    assert_eq!(changed.pads.as_ref().unwrap().0[0].id, part.pads.as_ref().unwrap().0[0].id);
    assert_eq!(changed.pads.as_ref().unwrap().0[0].net.as_deref(), Some("GND"));
    assert_eq!(call(&other, "import_footprint", args).await.is_error, Some(true));
    assert_eq!(engine.read(Clone::clone), applied);
    assert_eq!(call(&other, "undo", json!({})).await.is_error, Some(true));
    assert_eq!(call(&agent, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), snapshot.project);
    assert_eq!(call(&agent, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), applied);
    agent.cancel().await.unwrap();
    other.cancel().await.unwrap();
    server.shutdown();
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

#[tokio::test(flavor = "multi_thread")]
async fn observers_wait_and_resume_events_after_http_reconnect() {
    let engine = EngineHandle::new(Project::new("event HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let writer = connect(server.url(), "event-writer").await;
    let reader = connect(server.url(), "event-reader").await;
    let tools = reader.list_all_tools().await.unwrap();
    let schema = serde_json::to_value(&tools.iter().find(|tool| tool.name == "watch_events").unwrap().input_schema).unwrap();
    assert_eq!(schema, viamind_engine::input_schema::<viamind_engine::WatchEvents>());
    assert_eq!(schema["properties"]["timeout_ms"]["maximum"], 30000);
    assert_eq!(schema["properties"]["limit"]["maximum"], 1000);
    let baseline = call(&reader, "get_session_state", json!({})).await.structured_content.unwrap();
    assert_eq!(baseline["project"], serde_json::to_value(engine.read(Clone::clone)).unwrap());
    assert_eq!(baseline["selection"]["ids"], json!([]));

    // A pending HTTP read and a concurrent writer share one engine without blocking edits.
    let wait = call(&reader, "watch_events", json!({ "after": baseline["cursor"], "timeout_ms": 2000 }));
    let write = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        call(&writer, "add_part", resistor("R1", 10.0)).await
    };
    let (observed, created) = tokio::join!(wait, write);
    assert_eq!(observed.is_error, Some(false), "{observed:?}");
    let page = observed.structured_content.unwrap();
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["events"][0]["event"]["kind"], "design");
    assert!(page["events"][0]["event"]["data"]["origin"]["id"].as_str().unwrap().starts_with("event-writer#"));
    let id = created.structured_content.unwrap()["created"][0].clone();
    let cursor = page["cursor"].clone();
    reader.cancel().await.unwrap();

    // Writes while the reader is disconnected must be replayed to its new connection.
    call(&writer, "set_selection", json!({ "ids": [id] })).await;
    call(&writer, "delete_part", json!({ "id": id })).await;
    let reconnected = connect(server.url(), "event-reader").await;
    let first = call(&reconnected, "watch_events", json!({ "after": cursor, "limit": 2 })).await.structured_content.unwrap();
    assert_eq!(first["has_more"], true);
    assert_eq!(first["events"][0]["sequence"], 2);
    assert_eq!(first["events"][0]["event"]["kind"], "selection");
    assert_eq!(first["events"][0]["event"]["data"]["ids"], json!([id]));
    assert_eq!(first["events"][1]["sequence"], 3);
    assert_eq!(first["events"][1]["event"]["kind"], "design");
    // Repeating an unacknowledged cursor returns the same page.
    assert_eq!(call(&reconnected, "watch_events", json!({ "after": cursor, "limit": 2 })).await.structured_content.unwrap(), first);
    let last = call(&reconnected, "watch_events", json!({ "after": first["cursor"] })).await.structured_content.unwrap();
    assert_eq!(last["events"].as_array().unwrap().len(), 1);
    assert_eq!(last["events"][0]["sequence"], 4);
    assert_eq!(last["events"][0]["event"]["kind"], "selection");
    assert_eq!(last["events"][0]["event"]["data"]["ids"], json!([]));
    assert_eq!(last["has_more"], false);
    let timeout = call(&reconnected, "watch_events", json!({ "after": last["cursor"], "timeout_ms": 10 })).await.structured_content.unwrap();
    assert_eq!(timeout["events"], json!([]));
    assert_eq!(timeout["cursor"], last["cursor"]);
    assert_eq!(call(&reconnected, "get_session_state", json!({})).await.structured_content.unwrap()["cursor"], last["cursor"]);

    for (input, expected) in [
        (json!({ "limit": 0 }), "limit"),
        (json!({ "limit": 1001 }), "limit"),
        (json!({ "timeout_ms": 30001 }), "timeout_ms"),
        (json!({ "unknown": true }), "unknown"),
        (json!({ "after": { "stream_id": viamind_doc::ObjectId::new(), "sequence": 0 } }), "another session"),
        (json!({ "after": { "stream_id": last["cursor"]["stream_id"], "sequence": 100 } }), "future"),
    ] {
        let error = call(&reconnected, "watch_events", input).await;
        assert_eq!(error.is_error, Some(true), "{error:?}");
        assert!(format!("{:?}", error.content).contains(expected), "{error:?}");
    }
    assert_eq!(call(&reconnected, "get_session_state", json!({ "unknown": true })).await.is_error, Some(true));
    assert_eq!(call(&reconnected, "undo", json!({})).await.is_error, Some(true), "watching grants no undo ownership");
    reconnected.cancel().await.unwrap();
    writer.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_share_save_path_dirty_state_and_preserve_unapproved_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent board.viamind.json");
    let engine = EngineHandle::new(Project::new("save HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "save-a").await;
    let b = connect(server.url(), "save-b").await;
    let tools = a.list_all_tools().await.unwrap();
    let schema = serde_json::to_value(&tools.iter().find(|tool| tool.name == "save_project").unwrap().input_schema).unwrap();
    assert_eq!(schema, viamind_engine::input_schema::<viamind_engine::SaveProject>());
    assert_eq!(call(&a, "save_project", json!({})).await.is_error, Some(true));
    call(&a, "add_part", resistor("R1", 10.0)).await;
    assert_eq!(call(&b, "get_document_status", json!({})).await.structured_content.unwrap()["dirty"], true);
    let saved = call(&a, "save_project", json!({ "path": path })).await;
    assert_eq!(saved.is_error, Some(false), "{saved:?}");
    let status = call(&b, "get_document_status", json!({})).await.structured_content.unwrap();
    assert_eq!(status["path"], json!(path));
    assert_eq!(status["dirty"], false);
    let original = std::fs::read(&path).unwrap();
    call(&b, "add_part", resistor("R2", 20.0)).await;
    assert_eq!(call(&a, "save_project", json!({ "path": path })).await.is_error, Some(true));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(engine.document_status().dirty);
    assert_eq!(call(&b, "save_project", json!({})).await.is_error, Some(false));
    assert_eq!(viamind_doc::file::load(&path).unwrap(), engine.read(Clone::clone));
    assert!(!engine.document_status().dirty);
    // Saving does not become an undoable action or alter another agent's ownership.
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(false));
    assert!(engine.document_status().dirty);
    assert_eq!(call(&b, "redo", json!({})).await.is_error, Some(false));
    assert!(!engine.document_status().dirty);
    assert_eq!(call(&a, "save_project", json!({ "unknown": true })).await.is_error, Some(true));
    assert_eq!(call(&a, "get_document_status", json!({ "unknown": true })).await.is_error, Some(true));
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_replace_shared_documents_and_wake_observers_for_resynchronization() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lifecycle.viamind.json");
    let engine = EngineHandle::new(Project::new("original"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "lifecycle-a").await;
    let b = connect(server.url(), "lifecycle-b").await;
    call(&a, "add_part", resistor("R1", 10.0)).await;
    let before = call(&b, "get_session_state", json!({})).await.structured_content.unwrap();
    assert_eq!(call(&a, "new_project", json!({})).await.is_error, Some(true));
    assert_eq!(call(&a, "new_project", json!({ "discard_dirty": true })).await.is_error, Some(true));
    assert_eq!(call(&b, "get_session_state", json!({})).await.structured_content.unwrap(), before);
    let next = call(&a, "new_project", json!({ "name": "New board", "discard_dirty": true, "expected_cursor": before["cursor"] })).await;
    assert_eq!(next.is_error, Some(false), "{next:?}");
    let next = next.structured_content.unwrap();
    assert_eq!(next["project"]["name"], "New board");
    assert_eq!(call(&b, "get_design", json!({})).await.structured_content.unwrap(), next["project"]);
    assert_ne!(before["cursor"]["stream_id"], next["cursor"]["stream_id"]);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(true));
    assert_eq!(call(&a, "save_project", json!({ "path": path })).await.is_error, Some(false));
    // A pending observer wakes immediately when Open changes the stream epoch.
    let watch = call(&b, "watch_events", json!({ "after": next["cursor"], "timeout_ms": 3000 }));
    let open = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        call(&a, "open_project", json!({ "path": path, "expected_cursor": next["cursor"] })).await
    };
    let (observed, opened) =
        tokio::time::timeout(Duration::from_secs(2), async { tokio::join!(watch, open) }).await.expect("replacement wakes observer");
    assert_eq!(opened.is_error, Some(false), "{opened:?}");
    assert_eq!(observed.is_error, Some(true), "{observed:?}");
    assert!(format!("{:?}", observed.content).contains("resynchronize"));
    let current = call(&b, "get_session_state", json!({})).await.structured_content.unwrap();
    assert_eq!(current["project"], next["project"]);
    assert_eq!(call(&b, "get_document_status", json!({})).await.structured_content.unwrap()["path"], json!(path));
    assert_eq!(call(&a, "open_project", json!({ "path": path, "expected_cursor": next["cursor"] })).await.is_error, Some(true));
    assert_eq!(call(&a, "new_project", json!({ "unexpected": true })).await.is_error, Some(true));
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn property_updates_are_discoverable_shared_guarded_and_undoable() {
    let engine = EngineHandle::new(Project::new("Property HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "property-a").await;
    let b = connect(server.url(), "property-b").await;
    let tools = a.list_all_tools().await.unwrap();
    let schema = serde_json::to_value(&tools.iter().find(|tool| tool.name == "update_part").unwrap().input_schema).unwrap();
    assert_eq!(schema, viamind_engine::input_schema::<viamind_engine::UpdatePart>());
    let result = call(&a, "add_part", resistor("R1", 10.0)).await.structured_content.unwrap();
    let id = result["created"][0].clone();
    let original = call(&b, "query_parts", json!({ "ids": [id] })).await.structured_content.unwrap()["parts"][0].clone();
    assert_eq!(
        call(&b, "update_part", json!({ "id": id, "expected": original, "value": "22k", "rotation_deg": -90, "side": "bottom" })).await.is_error,
        Some(false)
    );
    let changed = call(&a, "query_parts", json!({ "ids": [id] })).await.structured_content.unwrap()["parts"][0].clone();
    assert_eq!(changed["value"], "22k");
    assert_eq!(changed["rotation_mdeg"], 270_000);
    assert_eq!(changed["side"], "bottom");
    assert_eq!(call(&a, "update_part", json!({ "id": id, "expected": original, "value": "stale" })).await.is_error, Some(true));
    assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(call(&a, "query_parts", json!({ "ids": [id] })).await.structured_content.unwrap()["parts"][0], original);
    assert_eq!(call(&b, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(call(&a, "query_parts", json!({ "ids": [id] })).await.structured_content.unwrap()["parts"][0], changed);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn board_outline_changes_are_shared_and_guarded_over_http() {
    let engine = EngineHandle::new(Project::new("Outline HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "outline-a").await;
    let b = connect(server.url(), "outline-b").await;
    let before = call(&b, "get_design", json!({})).await.structured_content.unwrap();
    let outline = json!({"outer":[{"x_mm":0,"y_mm":0},{"x_mm":40,"y_mm":0},{"x_mm":40,"y_mm":30},{"x_mm":0,"y_mm":30}],"cutouts":[]});
    assert_eq!(
        call(&a, "set_board_outline", json!({"outline":outline,"expected":{"project_id":before["id"],"outline":null}})).await.is_error,
        Some(false)
    );
    let after = call(&b, "get_design", json!({})).await.structured_content.unwrap();
    assert_eq!(after["board"]["outline"]["outer"][1]["x"], 40_000_000);
    assert_eq!(
        call(&b, "set_board_outline", json!({"outline":null,"expected":{"project_id":before["id"],"outline":null}})).await.is_error,
        Some(true)
    );
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(call(&b, "get_design", json!({})).await.structured_content.unwrap(), before);
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(call(&b, "get_design", json!({})).await.structured_content.unwrap(), after);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_share_persistent_pads_and_reject_stale_net_assignment() {
    let engine = EngineHandle::new(Project::new("Electrical MCP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "pad-editor").await;
    let b = connect(server.url(), "pad-observer").await;
    assert_eq!(call(&a, "add_part", resistor("R1", 10.0)).await.is_error, Some(false));
    let part = engine.read(|p| p.board.parts.values().next().unwrap().clone());
    let pad = json!({"number":"1","x_mm":-0.8,"y_mm":0,"width_mm":0.9,"height_mm":0.95,"net":"GND"});
    let input = json!({"id":part.id,"expected":part,"pads":[pad]});
    assert_eq!(call(&a, "set_part_pads", input.clone()).await.is_error, Some(false));
    let design = call(&b, "get_design", json!({})).await.structured_content.unwrap();
    assert_eq!(design["board"]["parts"][part.id.to_string()]["pads"][0]["net"], "GND");
    let after = engine.read(Clone::clone);
    assert_eq!(call(&b, "set_part_pads", input).await.is_error, Some(true));
    assert_eq!(engine.read(Clone::clone), after);
    assert_eq!(call(&a, "undo", json!({})).await.is_error, Some(false));
    assert!(engine.read(|p| p.board.part(&part.id).unwrap().pads.is_none()));
    assert_eq!(call(&a, "redo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), after);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn separate_agents_read_live_explicit_net_membership_without_creating_transactions() {
    let engine = EngineHandle::new(Project::new("Net HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let writer = connect(server.url(), "net-writer").await;
    let reader = connect(server.url(), "net-reader").await;
    for (name, x) in [("R1", 10.0), ("R2", 30.0)] {
        assert_eq!(call(&writer, "add_part", resistor(name, x)).await.is_error, Some(false));
        let part = engine.read(|p| p.board.part_by_designator(name).unwrap().clone());
        assert_eq!(
            call(
                &writer,
                "set_part_pads",
                json!({"id":part.id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":"GND"}]})
            )
            .await
            .is_error,
            Some(false)
        );
    }
    let state = engine.session_state();
    let nets = call(&reader, "query_nets", json!({})).await.structured_content.unwrap();
    assert_eq!(nets["nets"][0]["guide_count"], 1);
    let query = call(&reader, "query_connectivity", json!({"net":"GND","limit":1})).await;
    assert_eq!(query.is_error, Some(false));
    let page = query.structured_content.unwrap();
    assert_eq!(page["terminal_count"], 2);
    assert_eq!(page["guides"].as_array().unwrap().len(), 1);
    assert_eq!(engine.session_state(), state);
    assert_eq!(call(&reader, "query_connectivity", json!({"net":"GND","limit":0})).await.is_error, Some(true));
    assert_eq!(call(&reader, "query_connectivity", json!({})).await.is_error, Some(true));
    let id = engine.read(|p| p.board.part_by_designator("R1").unwrap().id);
    call(&writer, "move_part", json!({"id":id,"x_mm":40,"y_mm":20})).await;
    let moved = call(&reader, "query_connectivity", json!({"net":"GND"})).await.structured_content.unwrap();
    assert!(moved["terminals"].as_array().unwrap().iter().any(|p| p["x_mm"].as_f64() == Some(40.0)));
    assert_eq!(call(&writer, "undo", json!({})).await.is_error, Some(false));
    let restored = call(&reader, "query_connectivity", json!({"net":"GND"})).await.structured_content.unwrap();
    assert!(restored["terminals"].as_array().unwrap().iter().any(|p| p["x_mm"].as_f64() == Some(10.0)));
    writer.cancel().await.unwrap();
    reader.cancel().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn grouped_placement_rejects_a_stale_member_across_agents_and_undo_is_atomic() {
    let engine = EngineHandle::new(Project::new("Group HTTP"));
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let placer = connect(server.url(), "group-placer").await;
    let other = connect(server.url(), "group-other").await;
    call(&placer, "add_part", resistor("R1", 10.0)).await;
    call(&placer, "add_part", resistor("R2", 20.0)).await;
    let original = call(&placer, "get_design", json!({})).await.structured_content.unwrap();
    let expected = original["board"]["parts"].as_object().unwrap().values().cloned().collect::<Vec<_>>();
    let request = json!({"expected_project":original["id"],"expected":expected,"operation":{"kind":"translate","delta":{"x":1000000,"y":0}}});
    let id = expected[1]["id"].clone();
    call(&other, "move_part", json!({"id":id,"x_mm":30,"y_mm":20})).await;
    let state = engine.session_state();
    assert_eq!(call(&placer, "transform_parts", request).await.is_error, Some(true));
    assert_eq!(engine.session_state(), state);
    let design = call(&placer, "get_design", json!({})).await.structured_content.unwrap();
    let expected = design["board"]["parts"].as_object().unwrap().values().cloned().collect::<Vec<_>>();
    assert_eq!(
        call(&placer, "transform_parts", json!({"expected_project":design["id"],"expected":expected,"operation":{"kind":"flip"}})).await.is_error,
        Some(false)
    );
    assert!(engine.read(|p| p.board.parts.values().all(|p| p.side == viamind_doc::Side::Bottom)));
    assert_eq!(call(&placer, "undo", json!({})).await.is_error, Some(false));
    assert_eq!(engine.read(Clone::clone), state.project);
    placer.cancel().await.unwrap();
    other.cancel().await.unwrap();
    server.shutdown();
}
