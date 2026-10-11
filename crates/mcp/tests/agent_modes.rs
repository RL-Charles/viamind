use rmcp::{
    ClientHandler, RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    service::{NotificationContext, RunningService},
    transport::StreamableHttpClientTransport,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use viamind_engine::{
    EngineHandle, Origin,
    agent_policy::AgentMode,
    pcb_branches::{BranchActor, MergePcbBranch},
};
use viamind_mcp::{engine_registry, serve_http};
struct WatchingClient {
    changes: Arc<AtomicUsize>,
}
impl ClientHandler for WatchingClient {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(ClientCapabilities::default(), Implementation::new("same mode client", "0"))
    }
    async fn on_tool_list_changed(&self, _: NotificationContext<RoleClient>) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }
}
type Client = RunningService<RoleClient, WatchingClient>;
async fn client(url: String) -> (Client, Arc<AtomicUsize>) {
    let changes = Arc::new(AtomicUsize::new(0));
    let client = WatchingClient { changes: Arc::clone(&changes) }.serve(StreamableHttpClientTransport::from_uri(url)).await.unwrap();
    (client, changes)
}
async fn call(c: &Client, name: &str, input: Value) -> rmcp::model::CallToolResult {
    c.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(input.as_object().unwrap().clone())).await.unwrap()
}
fn value(result: rmcp::model::CallToolResult) -> Value {
    assert_eq!(result.is_error, Some(false));
    result.structured_content.unwrap()
}
async fn notification(changes: &AtomicUsize, after: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while changes.load(Ordering::SeqCst) <= after {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn actual_two_clients_refresh_tools_prepare_privately_and_only_human_accepts() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("protected.json");
    viamind_doc::file::save(&viamind_doc::Project::new("Modes"), &native).unwrap();
    let bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let server = serve_http(engine_registry(engine.clone()).unwrap(), "127.0.0.1:0".parse().unwrap()).await.unwrap();
    let (a, a_changes) = client(server.url()).await;
    let (b, b_changes) = client(server.url()).await;
    let live_tools = a.list_all_tools().await.unwrap();
    assert!(live_tools.iter().any(|tool| tool.name == "add_part"));
    assert_eq!(value(call(&b, "get_agent_mode", json!({})).await)["mode"], "live");
    value(call(&a, "add_part", json!({"designator":"LIVE","value":"human visible","footprint":"authored","x_mm":1,"y_mm":2})).await);
    let before = engine.session_state();
    let status = engine.document_status();
    let drc = engine.auto_drc_event_state();
    engine.agent_policy().set_mode(AgentMode::Approval).unwrap();
    notification(&a_changes, 0).await;
    notification(&b_changes, 0).await;
    let tools = a.list_all_tools().await.unwrap();
    for writer in [
        "add_part",
        "undo",
        "redo",
        "set_selection",
        "new_project",
        "open_project",
        "save_project",
        "configure_auto_drc",
        "set_agent_presence",
        "export_assembly_csv",
        "export_fabrication",
        "save_output_job",
        "run_output_job",
        "save_symbol_template",
        "export_symbol_template",
        "save_footprint_definition",
        "convert_footprint_file",
        "merge_pcb_branch",
    ] {
        assert!(!tools.iter().any(|tool| tool.name == writer), "{writer}");
        assert!(
            a.call_tool(
                CallToolRequestParams::new(writer)
                    .with_arguments(json!({"mode":"live","human":true,"readOnlyHint":true}).as_object().unwrap().clone())
            )
            .await
            .is_err(),
            "{writer}"
        );
    }
    assert_eq!(value(call(&a, "get_agent_mode", json!({})).await)["mode"], "approval");
    for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
        assert!(tools.iter().find(|tool| tool.name == name).unwrap().input_schema["properties"].get("path").is_none());
        assert_eq!(call(&b, name, json!({"path":null})).await.is_error, Some(true));
    }
    let branch = value(
        call(&a, "create_pcb_branch", json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"label":"human acceptance"})).await,
    );
    let id = branch["branch_id"].clone();
    let command = json!({"branch_id":id,"expected_revision":0,"commands":[{"name":"add_part","input":{"designator":"U1","value":"proposal","footprint":"authored","x_mm":3.000003,"y_mm":4.000007}}]});
    assert_eq!(call(&b, "apply_pcb_branch", command.clone()).await.is_error, Some(true));
    let applied = value(call(&a, "apply_pcb_branch", command).await);
    let created: viamind_doc::ObjectId = serde_json::from_value(applied["created"][0].clone()).unwrap();
    assert_eq!(value(call(&b, "get_pcb_branch_diff", json!({"branch_id":id})).await)["total"], 1);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(engine.auto_drc_event_state(), drc);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    let merging = json!({"branch_id":id,"expected_revision":1,"expected_project":before.project.id,"expected_cursor":before.cursor});
    let merge: MergePcbBranch = serde_json::from_value(merging.clone()).unwrap();
    assert!(a.call_tool(CallToolRequestParams::new("merge_pcb_branch").with_arguments(merging.as_object().unwrap().clone())).await.is_err());
    engine.merge_pcb_branch(merge, &BranchActor::human()).unwrap();
    assert!(engine.read(|p| p.board.parts.contains_key(&created)));
    assert_eq!(engine.event_cursor().sequence, before.cursor.sequence + 1);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before.project);
    engine.redo(Origin::User).unwrap();
    assert!(engine.read(|p| p.board.parts.contains_key(&created)));
    let old_a = a_changes.load(Ordering::SeqCst);
    let old_b = b_changes.load(Ordering::SeqCst);
    engine.agent_policy().set_mode(AgentMode::ReadOnly).unwrap();
    notification(&a_changes, old_a).await;
    notification(&b_changes, old_b).await;
    assert_eq!(value(call(&a, "get_agent_mode", json!({})).await)["mode"], "read_only");
    for name in ["create_pcb_branch", "apply_pcb_branch", "inspect_pcb_branch", "discard_pcb_branch", "merge_pcb_branch"] {
        assert!(b.call_tool(CallToolRequestParams::new(name).with_arguments(Default::default())).await.is_err());
    }
    let protected = engine.session_state();
    engine.agent_policy().set_mode(AgentMode::Live).unwrap();
    value(call(&b, "add_part", json!({"designator":"RESTORED","value":"live","footprint":"authored","x_mm":7,"y_mm":8})).await);
    assert_eq!(engine.event_cursor().sequence, protected.cursor.sequence + 1);
    assert_eq!(std::fs::read(&native).unwrap(), bytes);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    server.shutdown();
}

#[test]
fn actual_four_render_publications_survive_caller_abort_and_transport_runtime_shutdown() {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    use viamind_engine::agent_policy::AgentPolicyError;
    use viamind_mcp::CallContext;
    for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(name);
        let project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
        let sheet = *project.schematic.sheets.keys().next().unwrap();
        let engine = EngineHandle::new(project);
        let before = engine.session_state();
        let status = engine.document_status();
        let registry = engine_registry(engine.clone()).unwrap();
        let policy = engine.agent_policy();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let held_engine = engine.clone();
        let holder = std::thread::spawn(move || {
            held_engine.read(|_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
        let input = if name.starts_with("render_schematic") {
            json!({"sheet":sheet,"path":output,"options":{"width_px":128,"height_px":96}})
        } else {
            json!({"path":output,"options":{"width_px":128,"height_px":96}})
        };
        let transport = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(async move {
                let caller = tokio::spawn(async move { registry.call(name, input, CallContext::new("original renderer")).await });
                abort_rx.await.unwrap();
                caller.abort();
                assert!(caller.await.unwrap_err().is_cancelled());
            });
            drop(runtime);
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while policy.status().active_writers != 1 {
            assert!(Instant::now() < deadline, "{name} admission");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(policy.set_mode(AgentMode::Approval), Err(AgentPolicyError::Busy(1)));
        abort_tx.send(()).unwrap();
        transport.join().unwrap();
        assert!(!output.exists());
        assert_eq!(policy.set_mode(AgentMode::ReadOnly), Err(AgentPolicyError::Busy(1)));
        release_tx.send(()).unwrap();
        holder.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while policy.status().active_writers != 0 {
            assert!(Instant::now() < deadline, "{name} publication");
            std::thread::sleep(Duration::from_millis(1));
        }
        let bytes = std::fs::read(&output).unwrap();
        if name.ends_with("png") {
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        } else {
            assert!(std::str::from_utf8(&bytes).unwrap().contains("<svg"));
        }
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
        policy.set_mode(AgentMode::ReadOnly).unwrap();
    }
}
