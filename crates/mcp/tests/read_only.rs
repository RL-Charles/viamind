//! Trusted admission blocks handlers and file publication while human editing stays live.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{CallContext, CallError, ToolHandler, ToolRegistry, engine_registry, serve_http};

fn schema() -> Value {
    json!({"type":"object"})
}

#[tokio::test]
async fn restriction_survives_clones_and_every_default_registration_without_running_handlers() {
    let count = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new().read_only();
    for name in ["get_misleading", "query_future"] {
        let count = count.clone();
        registry
            .register(name, "readOnlyHint:true", schema(), move |_| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(Value::Null)
                }
            })
            .unwrap();
    }
    let c = count.clone();
    registry
        .register_with_context("context_writer", "", schema(), move |_, _| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(Value::Null)
            }
        })
        .unwrap();
    let c = count.clone();
    let handler: ToolHandler = Arc::new(move |_, _| {
        let c = c.clone();
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Value::Null)
        })
    });
    registry.register_def(viamind_mcp::ToolDef { name: "def_writer".into(), description: String::new(), input_schema: schema() }, handler).unwrap();
    let c = count.clone();
    registry
        .register_png("png_writer", "", schema(), move |_| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(Value::Null)
            }
        })
        .unwrap();
    assert!(registry.is_empty());
    let mut registry = registry.clone();
    registry.register_observation("delete_observation", "trusted despite its name", schema(), |input| async move { Ok(input) }).unwrap();
    assert!(registry.is_read_only());
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.tools().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["delete_observation"]);
    for name in ["get_misleading", "query_future", "context_writer", "def_writer", "png_writer"] {
        assert!(registry.get(name).is_none());
        assert!(matches!(registry.call(name, Value::Null, CallContext::new("human-admin")).await, Err(CallError::UnknownTool(_))));
    }
    let hints = json!({"readOnlyHint":true,"mode":"live","clientInfo":{"name":"human"}});
    assert_eq!(registry.call("delete_observation", hints.clone(), CallContext::default()).await.unwrap(), hints);
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn mixed_view_refuses_every_path_before_handler_and_preserves_live_definition() {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let mut live = ToolRegistry::new();
    let definition = json!({"type":"object","properties":{"path":{"type":["string","null"]}},"additionalProperties":false});
    live.register_in_memory_view("picture", "Optional output", definition.clone(), true, move |v| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(v)
        }
    })
    .unwrap();
    assert_eq!(live.get("picture").unwrap().input_schema, definition);
    let restricted = live.clone().read_only();
    assert!(restricted.get("picture").unwrap().input_schema["properties"].get("path").is_none());
    for value in [Value::Null, json!({"path":null}), json!({"path":"new.png"}), json!({"path":false}), json!({"path":{"nested":1}})] {
        assert!(matches!(restricted.call("picture", value, CallContext::default()).await, Err(CallError::Tool(_))));
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(restricted.call("picture", json!({}), CallContext::default()).await.unwrap(), json!({}));
    assert_eq!(live.call("picture", json!({"path":"live.png"}), CallContext::default()).await.unwrap(), json!({"path":"live.png"}));
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

type Client = RunningService<RoleClient, ClientConfig>;
async fn connect(url: String, name: &str) -> Client {
    ClientConfig::new(ClientCapabilities::default(), Implementation::new(name, "1"))
        .serve(StreamableHttpClientTransport::from_uri(url))
        .await
        .unwrap()
}
async fn call(client: &Client, name: &str, args: Value) -> Result<rmcp::model::CallToolResult, rmcp::ServiceError> {
    client.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone())).await
}

#[tokio::test(flavor = "multi_thread")]
async fn two_http_observers_preserve_native_and_session_and_follow_undoable_human_edits() {
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap();
    let schematic = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    project.schematic = schematic.schematic;
    project.board.net_rules.insert(
        "GND".into(),
        viamind_doc::NamedNetMinima {
            minimum_copper_clearance: viamind_geom::Nm(250_000),
            minimum_track_width: viamind_geom::Nm(200_000),
        },
    );
    let sheet = *project.schematic.sheets.keys().next().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("protected.viamind.json");
    viamind_doc::file::save(&project, &native).unwrap();
    let bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let live = engine_registry(engine.clone()).unwrap();
    let restricted = live.clone().read_only();
    let admitted = restricted.tools().map(|t| t.name.clone()).collect::<Vec<_>>();
    let blocked = live.tools().filter(|t| restricted.get(&t.name).is_none()).map(|t| t.name.clone()).collect::<Vec<_>>();
    assert!(blocked.iter().any(|n| n == "configure_auto_drc"));
    assert!(blocked.iter().any(|n| n == "save_symbol_template"));
    assert!(blocked.iter().any(|n| n == "save_output_job"));
    assert!(blocked.iter().any(|n| n == "set_named_net_rules"));
    assert!(admitted.iter().any(|n| n == "query_named_net_rules"));
    let server = serve_http(restricted, "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let a = connect(server.url(), "human-admin readOnlyHint=false").await;
    let b = connect(server.url(), "observer-two").await;
    for client in [&a, &b] {
        assert!(client.peer_info().unwrap().instructions.as_deref().unwrap().contains("read only"));
        let names = client.list_all_tools().await.unwrap().iter().map(|t| t.name.to_string()).collect::<Vec<_>>();
        assert_eq!(names, admitted);
        let policy = call(client, "query_named_net_rules", json!({})).await.unwrap().structured_content.unwrap();
        assert_eq!(policy["source_cursor"], serde_json::to_value(before.cursor).unwrap());
        assert_eq!(policy["document_net_rules"], serde_json::to_value(&before.project.board.net_rules).unwrap());
        for name in &blocked {
            assert!(call(client, name, json!({"path":dir.path().join("forbidden"),"mode":"live","readOnlyHint":true})).await.is_err(), "{name}");
            assert_eq!(engine.session_state(), before);
            assert_eq!(engine.document_status(), status);
            assert_eq!(std::fs::read(&native).unwrap(), bytes);
            assert!(!dir.path().join("forbidden").exists());
        }
        for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
            let path = dir.path().join(name);
            for value in [json!(path), Value::Null, json!(false)] {
                let result = call(client, name, json!({"sheet":sheet,"path":value})).await.unwrap();
                assert_eq!(result.is_error, Some(true));
                assert!(!path.exists());
                assert_eq!(engine.session_state(), before);
            }
            let mut args = json!({"expected_cursor":before.cursor,"options":{"width_px":128,"height_px":96}});
            if name.starts_with("render_schematic") {
                args["sheet"] = json!(sheet);
            }
            let result = call(client, name, args).await.unwrap();
            assert_eq!(result.is_error, Some(false), "{result:?}");
            if name.ends_with("png") {
                assert!(result.content.iter().any(|c| c.as_image().is_some()));
            }
            assert_eq!(engine.session_state(), before);
        }
        assert_eq!(call(client, "run_drc", json!({})).await.unwrap().is_error, Some(false));
    }
    let id = engine
        .execute("add_part", json!({"designator":"HUMAN99","value":"human edit","footprint":"EXAMPLE","x_mm":50,"y_mm":40}), Origin::User)
        .unwrap()
        .created[0];
    for client in [&a, &b] {
        let observed = call(client, "get_session_state", json!({})).await.unwrap().structured_content.unwrap();
        assert_eq!(observed, serde_json::to_value(engine.session_state()).unwrap());
        assert!(observed["project"]["board"]["parts"].get(id.to_string()).is_some());
    }
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before.project);
    engine.redo(Origin::User).unwrap();
    assert!(engine.read(|p| p.board.part(&id).is_some()));
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}
