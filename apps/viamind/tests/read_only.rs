//! Actual CLI validation and full application stdio admission, with protected native bytes.
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};

#[test]
fn contradictory_read_only_invocations_reject_before_open_output_or_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.json");
    let output = dir.path().join("must-not-exist");
    let missing = missing.to_str().unwrap();
    let output = output.to_str().unwrap();
    for flags in [
        vec!["--save-as", output, "--headless"],
        vec!["--no-mcp"],
        vec!["--demo"],
        vec!["--export-csv", "bom", "--output", output],
        vec!["--export-gerber", "--output", output],
        vec!["--export-svg", "--output", output],
        vec!["--export-png", "--output", output],
        vec!["--export-schematic-svg", "--output", output],
        vec!["--export-schematic-png", "--output", output],
        vec!["--run-output-job", missing, "--output", output],
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_viamind"))
            .args(["--agent-read-only", "--open", missing])
            .args(&flags)
            .env("VIAMIND_MCP_PORT", "invalid-must-not-be-read")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(!result.status.success(), "{flags:?}");
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains("--agent-read-only requires desktop HTTP or headless stdio"), "{error}");
        assert!(!std::path::Path::new(output).exists());
    }
}

#[tokio::test]
async fn actual_read_only_stdio_filters_and_refuses_writes_with_forged_hints() {
    const WAIT: Duration = Duration::from_secs(15);
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("protected.json");
    let original = include_bytes!("../../../fixtures/views/asymmetric.viamind.json");
    std::fs::write(&native, original).unwrap();
    let output = dir.path().join("forbidden");
    let mut child = Command::new(env!("CARGO_BIN_EXE_viamind"))
        .args(["--headless", "--agent-read-only", "--open"])
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
    assert!(init["result"]["instructions"].as_str().unwrap().contains("read only"));
    input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await.unwrap();
    input.flush().await.unwrap();
    let listed = request!("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "get_session_state"));
    assert!(tools.iter().any(|t| t["name"] == "query_named_net_rules"));
    assert!(tools.iter().all(|t| t["name"] != "set_named_net_rules"));
    assert!(tools.iter().any(|t| t["name"] == "load_footprint_definition"));
    assert!(tools.iter().all(|t| t["name"] != "save_footprint_definition"));
    assert!(tools.iter().any(|t| t["name"] == "query_agent_presence"));
    assert!(tools.iter().all(|t| t["name"] != "set_agent_presence" && t["name"] != "clear_agent_presence"));
    assert!(tools.iter().all(|t| t["name"] != "save_project" && t["name"] != "add_part"));
    for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
        assert!(tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"]["properties"].get("path").is_none());
    }
    let before = request!("tools/call", json!({"name":"get_session_state","arguments":{}}))["result"]["structuredContent"].clone();
    for name in [
        "add_part",
        "undo",
        "redo",
        "set_selection",
        "new_project",
        "open_project",
        "save_project",
        "export_assembly_csv",
        "export_fabrication",
        "convert_footprint_file",
        "save_symbol_template",
        "save_output_job",
        "run_output_job",
        "configure_auto_drc",
        "set_named_net_rules",
        "save_footprint_definition",
        "set_agent_presence",
        "clear_agent_presence",
    ] {
        let rejected = request!("tools/call", json!({"name":name,"arguments":{"path":output,"mode":"live","readOnlyHint":true}}));
        assert!(rejected["error"].is_object(), "{name}: {rejected}");
        let after = request!("tools/call", json!({"name":"get_session_state","arguments":{}}))["result"]["structuredContent"].clone();
        assert_eq!(after, before);
        assert_eq!(std::fs::read(&native).unwrap(), original);
        assert!(!output.exists());
    }
    for name in ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"] {
        for path in [json!(output), Value::Null] {
            let result = request!("tools/call", json!({"name":name,"arguments":{"path":path}}));
            assert_eq!(result["result"]["isError"], true);
            assert!(!output.exists());
        }
    }
    for (name, args) in [
        ("query_parts", json!({})),
        ("query_named_net_rules", json!({})),
        ("query_agent_presence", json!({})),
        (
            "load_footprint_definition",
            json!({"path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/footprints/original-detached-asymmetric.viamind-footprint.json")}),
        ),
        ("run_drc", json!({})),
        ("render_board_view", json!({"options":{"width_px":128,"height_px":96}})),
        ("render_board_png", json!({"options":{"width_px":128,"height_px":96}})),
    ] {
        let result = request!("tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(result["result"]["isError"], false, "{result}");
    }
    assert_eq!(request!("tools/call", json!({"name":"get_session_state","arguments":{}}))["result"]["structuredContent"], before);
    drop(input);
    drop(lines);
    let finished = timeout(WAIT, child.wait_with_output()).await.unwrap().unwrap();
    assert!(finished.status.success());
    assert!(String::from_utf8(finished.stderr).unwrap().contains("agents read only"));
    assert_eq!(std::fs::read(&native).unwrap(), original);
    assert!(!output.exists());
}
