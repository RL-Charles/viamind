use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{CallContext, engine_registry, serve_http};

type Client = RunningService<RoleClient, ClientConfig>;
async fn client(url: String) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new("same #1 human-admin", "1"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(client: &Client, name: &str, args: Value) -> rmcp::model::CallToolResult {
    client.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_same_named_clients_own_distinct_markers_and_cannot_spoof_peers_or_edit_read_only_presence() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().join("protected.json");
    viamind_doc::file::save(&project, &native).unwrap();
    let bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let id = *project.board.parts.keys().next().unwrap();
    let registry = engine_registry(engine.clone()).unwrap();
    let input = json!({"expected_project":project.id,"expected_stream":before.cursor.stream_id,"cursor":{"x":1,"y":-3},"selection":[id],"working_on":"Checking route"});
    assert!(registry.call("set_agent_presence", input.clone(), CallContext::new("human-admin#1")).await.is_err());
    let server = serve_http(registry.clone(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = client(server.url()).await;
    let b = client(server.url()).await;
    assert_eq!(call(&a, "set_agent_presence", input.clone()).await.is_error, Some(false));
    let pair = call(&b, "set_agent_presence", input.clone()).await.structured_content.unwrap();
    let agents = pair["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);
    assert_ne!(agents[0]["connection"], agents[1]["connection"]);
    assert_eq!(agents[0]["display_name"], agents[1]["display_name"]);
    let guard = json!({"expected_project":project.id,"expected_stream":before.cursor.stream_id});
    let mut spoof = guard.clone();
    spoof["connection"] = agents[1]["connection"].clone();
    assert_eq!(call(&a, "clear_agent_presence", spoof).await.is_error, Some(true));
    assert_eq!(engine.query_agent_presence().agents.len(), 2);
    assert_eq!(call(&a, "clear_agent_presence", guard.clone()).await.is_error, Some(false));
    assert_eq!(engine.query_agent_presence().agents.len(), 1);
    let retained = engine.query_agent_presence().agents[0].clone();
    for invalid in [
        {
            let mut v = input.clone();
            v["selection"] = json!(vec![Value::Null; 65]);
            v
        },
        {
            let mut v = input.clone();
            v["working_on"] = json!("x".repeat(257));
            v["cursor"] = Value::Null;
            v
        },
        {
            let mut v = input.clone();
            v["cursor"]["x"] = json!(10_000_000_001_i64);
            v
        },
        {
            let mut v = input.clone();
            v["cursor"]["extra"] = json!(true);
            v
        },
        {
            let mut v = input.clone();
            v["selection"] = json!([id, id]);
            v
        },
        {
            let mut v = input.clone();
            v["selection"] = json!([viamind_doc::ObjectId::new()]);
            v
        },
    ] {
        assert_eq!(call(&b, "set_agent_presence", invalid).await.is_error, Some(true));
        let after = engine.query_agent_presence().agents[0].clone();
        assert_eq!(after.connection, retained.connection);
        assert_eq!(after.cursor, retained.cursor);
        assert_eq!(after.working_on, retained.working_on);
        assert!(after.remaining_lease_ms <= retained.remaining_lease_ms);
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
    }
    let observer_server = serve_http(registry.read_only(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let observer = client(observer_server.url()).await;
    let tools = observer.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "query_agent_presence"));
    for name in ["set_agent_presence", "clear_agent_presence"] {
        assert!(tools.iter().all(|t| t.name != name));
        assert!(observer.call_tool(CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone())).await.is_err());
    }
    assert_eq!(call(&observer, "query_agent_presence", json!({})).await.structured_content.unwrap()["agents"].as_array().unwrap().len(), 1);
    assert_eq!(engine.session_state(), before);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    // Human changes and undo still work, and deleting selected geometry only hides that ID.
    engine.execute("delete_part", json!({"id":id}), Origin::User).unwrap();
    assert!(engine.query_agent_presence().agents[0].selection.is_empty());
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before.project);
    assert_eq!(engine.query_agent_presence().agents[0].selection, [id]);
    assert_eq!(call(&b, "clear_agent_presence", guard).await.is_error, Some(false));
    assert!(engine.query_agent_presence().agents.is_empty());
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    observer.cancel().await.unwrap();
    server.shutdown();
    observer_server.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_sheet_scoped_clients_switch_and_reject_foreign_ids_without_design_changes() {
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    project.board = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap().board;
    let sheet = project.schematic.sheets.values().next().unwrap().clone();
    let content = sheet.content.as_ref().unwrap();
    let symbol = content.symbols.values().next().unwrap();
    let ids =
        vec![symbol.id, *content.wires.keys().next().unwrap(), *content.junctions.keys().next().unwrap(), *content.labels.keys().next().unwrap()];
    let board_id = *project.board.parts.keys().next().unwrap();
    let empty = viamind_doc::Sheet { id: viamind_doc::ObjectId::new(), name: "Empty attention".into(), content: None };
    project.schematic.sheets.insert(empty.id, empty.clone());
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().join("protected.json");
    viamind_doc::file::save(&project, &native).unwrap();
    let bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let registry = engine_registry(engine.clone()).unwrap();
    let server = serve_http(registry.clone(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = client(server.url()).await;
    let b = client(server.url()).await;
    let pcb = json!({"expected_project":project.id,"expected_stream":before.cursor.stream_id,"cursor":{"x":0,"y":0},"selection":[board_id],"working_on":"PCB attention"});
    let mut sch = pcb.clone();
    sch["surface"] = json!({"kind":"schematic","sheet":sheet.id});
    sch["selection"] = json!(ids);
    sch["working_on"] = json!("Schematic attention");
    let first = call(&a, "set_agent_presence", pcb.clone()).await.structured_content.unwrap();
    assert!(first["agents"][0].get("surface").is_none());
    let pair = call(&b, "set_agent_presence", sch.clone()).await.structured_content.unwrap();
    assert_eq!(pair["agents"].as_array().unwrap().len(), 2);
    let b_owner = pair["agents"].as_array().unwrap().iter().find(|v| v.get("surface").is_some()).unwrap()["connection"].as_u64().unwrap();
    let prior = engine.query_agent_presence();
    for (index, mut invalid) in [sch.clone(), sch.clone(), sch.clone(), sch.clone(), sch.clone(), sch.clone(), sch.clone()].into_iter().enumerate() {
        match index {
            0 => invalid["surface"] = Value::Null,
            1 => invalid["surface"] = json!({"kind":"pcb","sheet":sheet.id}),
            2 => invalid["surface"] = json!({"kind":"schematic","sheet":sheet.id,"extra":true}),
            3 => invalid["selection"] = json!([board_id]),
            4 => invalid["selection"] = json!([symbol.pins[0].id]),
            5 => invalid["surface"] = json!({"kind":"schematic","sheet":empty.id}),
            _ => invalid["selection"] = json!(vec![Value::Null; 65]),
        }
        assert_eq!(call(&b, "set_agent_presence", invalid).await.is_error, Some(true));
        let after = engine.query_agent_presence();
        for (old, new) in prior.agents.iter().zip(&after.agents) {
            assert_eq!(old.surface, new.surface);
            assert_eq!(old.selection, new.selection);
            assert_eq!(old.cursor, new.cursor);
            assert!(new.remaining_lease_ms <= old.remaining_lease_ms);
        }
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
    }
    let mut empty_attention = sch.clone();
    empty_attention["surface"] = json!({"kind":"schematic","sheet":empty.id});
    empty_attention["selection"] = json!([]);
    assert_eq!(call(&a, "set_agent_presence", empty_attention).await.is_error, Some(false));
    assert_eq!(engine.query_agent_presence().agents.len(), 2);
    assert!(engine.query_agent_presence().agents.iter().all(|v| matches!(v.surface, viamind_engine::PresenceSurface::Schematic { .. })));
    let observer_server = serve_http(registry.read_only(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let observer = client(observer_server.url()).await;
    let observed = call(&observer, "query_agent_presence", json!({})).await.structured_content.unwrap();
    assert_eq!(observed["agents"].as_array().unwrap().len(), 2);
    assert_eq!(engine.session_state(), before);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    engine
        .execute(
            "edit_schematic_sheet",
            json!({"expected_project":project.id,"sheet_id":sheet.id,"expected":sheet,"name":sheet.name,"edits":[{"kind":"delete","id":symbol.id}]}),
            Origin::User,
        )
        .unwrap();
    assert!(!engine.query_agent_presence().agents.iter().find(|v| v.connection == b_owner).unwrap().selection.contains(&symbol.id));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.query_agent_presence().agents.iter().find(|v| v.connection == b_owner).unwrap().selection, ids);
    engine.execute("delete_schematic_sheet", json!({"expected_project":project.id,"expected":empty}), Origin::User).unwrap();
    assert_eq!(engine.query_agent_presence().agents.len(), 1);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.query_agent_presence().agents.len(), 1);
    assert_eq!(engine.read(Clone::clone), project);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    observer.cancel().await.unwrap();
    server.shutdown();
    observer_server.shutdown();
}
