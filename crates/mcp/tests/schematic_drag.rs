use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::RunningService,
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use viamind_engine::{DragSchematicSymbol, EngineHandle, Origin, agent_policy::AgentMode};
use viamind_mcp::{agent::CallContext, engine_registry, serve_http};
type Client = RunningService<RoleClient, ClientConfig>;
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn fixture() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/connected-drag.viamind.json")).unwrap())
}
fn input(e: &EngineHandle) -> Value {
    let s = e.session_state();
    json!({"expected_project":s.project.id,"source_cursor":s.cursor,"sheet_id":"00000000-0000-0000-0000-0000000001f6","symbol_id":"00000000-0000-0000-0000-0000000001f7","delta":{"x":3000000,"y":5000000}})
}
async fn call(c: &Client, name: &'static str, args: Value) -> rmcp::model::CallToolResult {
    c.call_tool(CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone())).await.unwrap()
}
#[tokio::test(flavor = "multi_thread")]
async fn initialized_two_client_preview_guard_apply_and_native_invariants_in_all_modes() {
    let _serial = SERIAL.lock().await;
    for mode in [AgentMode::Live, AgentMode::Approval, AgentMode::ReadOnly] {
        let engine = fixture();
        let directory = tempfile::tempdir().unwrap();
        let native = directory.path().join("active.viamind.json");
        engine.save_project(viamind_engine::SaveProject { path: Some(native.clone()), overwrite: false }).unwrap();
        let bytes = std::fs::read(&native).unwrap();
        if mode == AgentMode::ReadOnly {
            engine.agent_policy().lock_read_only().unwrap();
        } else {
            engine.agent_policy().set_mode(mode).unwrap();
        }
        let args = input(&engine);
        let expected =
            engine.prepare_schematic_symbol_drag(serde_json::from_value::<DragSchematicSymbol>(args.clone()).unwrap()).unwrap().run_value().unwrap();
        let before = engine.session_state();
        let status = engine.document_status();
        let drc = engine.auto_drc_event_state();
        let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
        let a = ClientConfig::new(ClientCapabilities::default(), Implementation::new("same mover", "1"))
            .serve(StreamableHttpClientTransport::from_uri(server.url()))
            .await
            .unwrap();
        let b = ClientConfig::new(ClientCapabilities::default(), Implementation::new("same mover", "1"))
            .serve(StreamableHttpClientTransport::from_uri(server.url()))
            .await
            .unwrap();
        let tools = a.list_all_tools().await.unwrap();
        assert!(tools.iter().any(|t| t.name == "preview_schematic_symbol_drag"));
        assert_eq!(tools.iter().any(|t| t.name == "drag_schematic_symbol"), mode == AgentMode::Live);
        for client in [&a, &b] {
            let r = call(client, "preview_schematic_symbol_drag", args.clone()).await;
            assert_eq!(r.is_error, Some(false));
            assert_eq!(r.structured_content.unwrap(), expected);
        }
        assert_eq!(expected["added_wire_count"], 3);
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
        assert_eq!(engine.auto_drc_event_state(), drc);
        let mut invalid = args.clone();
        invalid["pass"] = json!(true);
        assert_eq!(call(&a, "preview_schematic_symbol_drag", invalid).await.is_error, Some(true));
        assert_eq!(engine.session_state(), before);
        if mode != AgentMode::Live {
            assert!(
                a.call_tool(CallToolRequestParams::new("drag_schematic_symbol").with_arguments(args.as_object().unwrap().clone())).await.is_err()
            );
            assert_eq!(engine.session_state(), before);
        } else {
            assert_eq!(call(&a, "add_part", json!({"designator":"R9","value":"","footprint":"F","x_mm":0,"y_mm":0})).await.is_error, Some(false));
            let peer = engine.session_state();
            assert_eq!(call(&b, "drag_schematic_symbol", args.clone()).await.is_error, Some(true));
            assert_eq!(engine.session_state(), peer);
            engine.undo(Origin::User).unwrap();
            assert_eq!(engine.session_state().project, before.project);
            let fresh = input(&engine);
            let source = engine.event_cursor();
            assert_eq!(call(&b, "preview_schematic_symbol_drag", fresh.clone()).await.is_error, Some(false));
            assert_eq!(call(&b, "drag_schematic_symbol", fresh).await.is_error, Some(false));
            let after = engine.session_state();
            assert_eq!(after.cursor.sequence, source.sequence + 1);
            assert_eq!(after.project.board, before.project.board);
            assert_eq!(call(&b, "undo", json!({})).await.is_error, Some(false));
            assert_eq!(engine.session_state().project, before.project);
        }
        assert_eq!(std::fs::read(&native).unwrap(), bytes);
        assert_eq!(engine.document_status().path, status.path);
        a.cancel().await.unwrap();
        b.cancel().await.unwrap();
        server.shutdown();
    }
}

#[test]
fn cancelled_actual_preview_awaiter_retains_queued_worker_slot_until_retirement() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).max_blocking_threads(1).build().unwrap();
    runtime.block_on(async {
        let _serial = SERIAL.lock().await;
        let engine = fixture();
        let args = input(&engine);
        let typed = serde_json::from_value::<DragSchematicSymbol>(args.clone()).unwrap();
        let held = engine.prepare_schematic_symbol_drag(typed).unwrap();
        let registry = engine_registry(engine.clone()).unwrap();
        let (start, ready) = std::sync::mpsc::sync_channel(1);
        let (release, wait) = std::sync::mpsc::sync_channel(1);
        struct Release(Option<std::sync::mpsc::SyncSender<()>>);
        impl Drop for Release {
            fn drop(&mut self) {
                if let Some(s) = self.0.take() {
                    let _ = s.send(());
                }
            }
        }
        let mut guard = Release(Some(release));
        let blocker = tokio::task::spawn_blocking(move || {
            start.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.recv().unwrap();
        let pending = tokio::spawn(async move { registry.call("preview_schematic_symbol_drag", args, CallContext::new("cancelled mover")).await });
        let mut admitted = false;
        for _ in 0..200 {
            match engine.prepare_schematic_symbol_drag(typed) {
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
        assert!(engine.prepare_schematic_symbol_drag(typed).is_err());
        guard.0.take().unwrap().send(()).unwrap();
        blocker.await.unwrap();
        drop(held);
        for _ in 0..200 {
            let first = engine.prepare_schematic_symbol_drag(typed).unwrap();
            if let Ok(second) = engine.prepare_schematic_symbol_drag(typed) {
                drop(second);
                drop(first);
                return;
            }
            drop(first);
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        panic!("queued preview must retire and release its actual worker slot");
    });
}
