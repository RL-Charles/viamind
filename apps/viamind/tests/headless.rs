//! A real agent process session with no display, using the full application binary.
use std::{path::Path, process::Stdio, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, Origin};

const TIMEOUT: Duration = Duration::from_secs(15);

struct Client {
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    sequence: u64,
}

impl Client {
    async fn send(&mut self, value: Value) {
        self.stdin.write_all(format!("{value}\n").as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.sequence += 1;
        let id = self.sequence;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await;
        loop {
            let line = timeout(TIMEOUT, self.stdout.next_line()).await.expect("response timeout").unwrap().expect("stdout closed early");
            let message: Value = serde_json::from_str(&line).expect("only JSON-RPC on stdout");
            if message.get("id") == Some(&json!(id)) && message.get("method").is_none() {
                assert!(message.get("error").is_none(), "{message}");
                return message["result"].clone();
            }
        }
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.request("tools/call", json!({ "name": name, "arguments": arguments })).await;
        assert_eq!(result["isError"], false, "{result}");
        result["structuredContent"].clone()
    }
}

async fn spawn(input: Option<&Path>, output: &Path) -> (Child, Client) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_viamind"));
    command.args(["--headless", "--save-as"]).arg(output);
    if let Some(path) = input {
        command.arg("--open").arg(path);
    }
    let mut child = command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("VIAMIND_MCP_PORT", "unused-in-headless")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn headless app");
    let mut client = Client { stdin: child.stdin.take().unwrap(), stdout: BufReader::new(child.stdout.take().unwrap()).lines(), sequence: 0 };
    let info = client
        .request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": { "name": "headless-agent", "version": "1.0" }
            }),
        )
        .await;
    assert_eq!(info["serverInfo"]["name"], "viamind");
    client.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    (child, client)
}

#[tokio::test]
async fn full_engine_edits_without_display_and_saves_on_client_eof() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input with spaces.viamind.json");
    let output = directory.path().join("result.viamind.json");
    let engine = EngineHandle::new(Project::new("Power board"));
    engine.execute("add_part", json!({ "designator": "R1", "value": "1k", "footprint": "R_0603", "x_mm": 0, "y_mm": 0 }), Origin::User).unwrap();
    engine.read(|project| file::save(project, &input)).unwrap();
    let original_bytes = std::fs::read(&input).unwrap();
    let original = engine.read(|project| serde_json::to_value(project).unwrap());
    let (mut child, mut client) = spawn(Some(&input), &output).await;
    let tools = client.request("tools/list", json!({})).await;
    let names: Vec<_> = tools["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    for name in [
        "add_part",
        "move_part",
        "update_part",
        "delete_part",
        "undo",
        "redo",
        "query_parts",
        "get_design",
        "get_selection",
        "set_selection",
        "get_session_state",
        "watch_events",
        "get_document_status",
        "save_project",
    ] {
        assert!(names.contains(&name), "missing {name}");
    }
    assert_eq!(client.tool("get_design", json!({})).await, original);
    let loaded_status = client.tool("get_document_status", json!({})).await;
    assert_eq!(loaded_status["path"], json!(input));
    assert_eq!(loaded_status["dirty"], false);
    let baseline = client.tool("get_session_state", json!({})).await;
    assert_eq!(baseline["project"], original);
    let added = client.tool("add_part", json!({ "designator": "R2", "value": "10k", "footprint": "R_0603", "x_mm": 1.234567, "y_mm": -2 })).await;
    let id = added["created"][0].clone();
    assert_eq!(client.tool("get_document_status", json!({})).await["dirty"], true);
    let page = client.tool("query_parts", json!({ "value": "10k" })).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["parts"][0]["position"], json!({ "x": 1_234_567, "y": -2_000_000 }));
    client.tool("set_selection", json!({ "ids": [id] })).await;
    let selection = client.tool("get_selection", json!({})).await;
    assert!(selection["origin"]["id"].as_str().unwrap().starts_with("headless-agent#"));
    client.tool("undo", json!({})).await;
    assert_eq!(client.tool("get_design", json!({})).await, original);
    assert_eq!(client.tool("get_selection", json!({})).await["ids"], json!([]));
    client.tool("redo", json!({})).await;
    client.tool("update_part", json!({ "id": id, "value": "47k", "rotation_deg": 45, "side": "bottom" })).await;
    let expected = client.tool("get_design", json!({})).await;
    let events = client.tool("watch_events", json!({ "after": baseline["cursor"] })).await;
    let kinds: Vec<_> = events["events"].as_array().unwrap().iter().map(|event| event["event"]["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["design", "selection", "design", "selection", "design", "design"]);
    assert_eq!(events["cursor"]["sequence"], 6);
    let explicit = directory.path().join("tool-saved.viamind.json");
    let saved = client.tool("save_project", json!({ "path": explicit })).await;
    assert_eq!(saved["dirty"], false);
    assert_eq!(saved["saved_cursor"], events["cursor"]);
    assert_eq!(serde_json::to_value(file::load(&explicit).unwrap()).unwrap(), expected, "tool writes before client EOF");
    assert!(!output.exists(), "save is after clean session EOF");
    drop(client);
    let status = timeout(TIMEOUT, child.wait()).await.expect("headless exit timeout").unwrap();
    assert!(status.success(), "{status}");
    assert_eq!(serde_json::to_value(file::load(&output).unwrap()).unwrap(), expected);
    assert_eq!(std::fs::read(&input).unwrap(), original_bytes, "input file untouched");
}

#[tokio::test]
async fn headless_agent_can_create_save_and_reopen_in_one_connection() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("final.json");
    let native = directory.path().join("created.viamind.json");
    let (mut child, mut client) = spawn(None, &output).await;
    let created = client.tool("new_project", json!({ "name": "Headless lifecycle" })).await;
    client.tool("save_project", json!({ "path": native })).await;
    client.tool("new_project", json!({ "name": "Temporary" })).await;
    let reopened = client.tool("open_project", json!({ "path": native })).await;
    assert_eq!(reopened["project"], created["project"]);
    assert_ne!(reopened["cursor"]["stream_id"], created["cursor"]["stream_id"]);
    assert_eq!(client.tool("get_document_status", json!({})).await["dirty"], false);
    drop(client);
    assert!(timeout(TIMEOUT, child.wait()).await.unwrap().unwrap().success());
    assert_eq!(serde_json::to_value(file::load(output).unwrap()).unwrap(), created["project"]);
}

#[tokio::test]
async fn output_write_failure_returns_nonzero_after_an_empty_design_session() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("missing-parent/result.viamind.json");
    let (child, mut client) = spawn(None, &output).await;
    assert_eq!(client.tool("get_design", json!({})).await["name"], "Untitled");
    drop(client);
    let result = timeout(TIMEOUT, child.wait_with_output()).await.unwrap().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("could not save project"));
    assert!(!output.exists());
}

#[test]
fn usage_and_invalid_options_return_without_initializing_a_display() {
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_viamind")).args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").output().unwrap()
    };
    for args in [vec!["--help"], vec!["--version"]] {
        let result = run(&args);
        assert!(result.status.success());
        assert!(!result.stdout.is_empty());
    }
    for args in [
        vec!["--bogus"],
        vec!["--headless", "--no-mcp"],
        vec!["--headless", "--headless"],
        vec!["--open"],
        vec!["--save-as"],
        vec!["--open", "--headless"],
        vec!["--demo", "--headless"],
        vec!["--save-as", "result.json"],
        vec!["--help", "--open", "input.json"],
    ] {
        let result = run(&args);
        assert!(!result.status.success(), "{args:?}");
        assert!(result.stdout.is_empty(), "no protocol garbage or help on error");
        assert!(!result.stderr.is_empty());
    }
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_viamind"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("VIAMIND_MCP_PORT", "bad-port")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("VIAMIND_MCP_PORT"));
}

#[test]
fn invalid_native_input_is_reported_and_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("invalid.json");
    std::fs::write(&input, "{}").unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_viamind"))
        .args(["--headless", "--open"])
        .arg(&input)
        .arg("--save-as")
        .arg(&input)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("could not open project"));
    assert_eq!(std::fs::read_to_string(input).unwrap(), "{}");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_viamind"))
        .args(["--headless", "--open"])
        .arg(directory.path().join("missing.json"))
        .output()
        .unwrap();
    assert!(!result.status.success());
}
