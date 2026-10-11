//! Actual CLI validation and full application stdio admission, with protected native bytes.
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};

#[tokio::test]
async fn actual_approval_stdio_prepares_private_candidate_without_live_or_file_writes() {
    const WAIT: Duration = Duration::from_secs(15);
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("protected.json");
    let original = include_bytes!("../../../fixtures/views/asymmetric.viamind.json");
    std::fs::write(&native, original).unwrap();
    let output = dir.path().join("forbidden");
    let mut child = Command::new(env!("CARGO_BIN_EXE_viamind"))
        .args(["--headless", "--agent-approval", "--open"])
        .arg(&native)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("VIAMIND_MCP_PORT", "ignored-headless")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut sequence = 0;
    macro_rules! request { ($method:expr,$params:expr) => {{
        sequence+=1;
        input.write_all(format!("{}\n",json!({"jsonrpc":"2.0","id":sequence,"method":$method,"params":$params})).as_bytes()).await.unwrap();
        input.flush().await.unwrap();
        loop {
            let line=timeout(WAIT,lines.next_line()).await.unwrap().unwrap().unwrap();
            let value:Value=serde_json::from_str(&line).unwrap();
            if value["id"]==json!(sequence) && value.get("method").is_none() {break value;}
        }
    }}; }
    let init =
        request!("initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"human-admin-live","version":"1"}}));
    assert!(init["result"]["instructions"].as_str().unwrap().contains("Approval mode"));
    input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await.unwrap();
    input.flush().await.unwrap();
    let listed = request!("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    for allowed in ["get_agent_mode", "get_session_state", "create_pcb_branch", "apply_pcb_branch", "inspect_pcb_branch", "discard_pcb_branch"] {
        assert!(tools.iter().any(|tool| tool["name"] == allowed), "{allowed}");
    }
    for hidden in ["add_part", "merge_pcb_branch", "save_project", "undo", "set_agent_presence", "configure_auto_drc"] {
        assert!(tools.iter().all(|tool| tool["name"] != hidden));
        assert!(request!("tools/call", json!({"name":hidden,"arguments":{"human":true,"mode":"live"}}))["error"].is_object());
    }
    let mode = request!("tools/call", json!({"name":"get_agent_mode","arguments":{}}))["result"]["structuredContent"].clone();
    assert_eq!(mode["mode"], "approval");
    assert_eq!(mode["startup_locked"], false);
    let before = request!("tools/call", json!({"name":"get_session_state","arguments":{}}))["result"]["structuredContent"].clone();
    let branch=request!("tools/call",json!({"name":"create_pcb_branch","arguments":{"expected_project":before["project"]["id"],"expected_cursor":before["cursor"],"label":"CLI private candidate"}}))["result"]["structuredContent"].clone();
    let id = branch["branch_id"].clone();
    assert!(id.is_string(), "{branch}");
    let applied = request!(
        "tools/call",
        json!({"name":"apply_pcb_branch","arguments":{"branch_id":id,"expected_revision":0,"commands":[{"name":"add_part","input":{"designator":"CLI_PROPOSAL","value":"candidate","footprint":"authored","x_mm":3.000003,"y_mm":4.000007}}]}})
    );
    assert_eq!(applied["result"]["isError"], false, "{applied}");
    let diff = request!("tools/call", json!({"name":"get_pcb_branch_diff","arguments":{"branch_id":id}}));
    assert_eq!(diff["result"]["structuredContent"]["total"], 1);
    for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
        assert_eq!(request!("tools/call", json!({"name":name,"arguments":{"path":output}}))["result"]["isError"], true);
    }
    assert_eq!(request!("tools/call", json!({"name":"get_session_state","arguments":{}}))["result"]["structuredContent"], before);
    let rejected = request!("tools/call", json!({"name":"discard_pcb_branch","arguments":{"branch_id":id,"expected_revision":1}}));
    assert_eq!(rejected["result"]["isError"], false, "{rejected}");
    assert_eq!(std::fs::read(&native).unwrap(), original);
    assert!(!output.exists());
    drop(input);
    drop(lines);
    let finished = timeout(WAIT, child.wait_with_output()).await.unwrap().unwrap();
    assert!(finished.status.success());
    assert!(String::from_utf8(finished.stderr).unwrap().contains("trusted human desktop acceptance required"));
    assert_eq!(std::fs::read(&native).unwrap(), original);
    assert!(!output.exists());
}

#[test]
fn incompatible_startup_modes_refuse_before_opening_input_or_display() {
    let directory = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_viamind"))
        .args(["--agent-read-only", "--agent-approval", "--open"])
        .arg(directory.path().join("missing.json"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("VIAMIND_MCP_PORT", "invalid-before-endpoint")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8(result.stderr).unwrap().contains("choose only one of --agent-read-only and --agent-approval"));
}
