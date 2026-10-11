//! Spawns the `viamind-mcp` binary and speaks raw JSON-RPC to it over stdio,
//! the way an MCP client such as Claude Code does.

use std::{process::Stdio, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{ChildStdin, ChildStdout, Command},
    time::timeout,
};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Newest MCP revision that still has the `initialize` handshake.
const PROTOCOL_VERSION: &str = "2025-11-25";

struct Client {
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl Client {
    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
    }

    /// Send a request and return the response with the same id.
    async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let line = timeout(TIMEOUT, self.stdout.next_line())
                .await
                .expect("timed out waiting for the server")
                .unwrap()
                .expect("server closed stdout");
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("server wrote non-JSON to stdout ({e}): {line}"));
            // Skip server notifications and requests; we only want our response.
            if message.get("id") == Some(&json!(id)) && message.get("method").is_none() {
                return message;
            }
        }
    }
}

#[tokio::test]
async fn initialize_list_and_call_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_viamind-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn viamind-mcp");
    let mut client = Client {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()).lines(),
    };

    // initialize
    let init = client
        .request(
            1,
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "viamind-test-client", "version": "0.0.0" }
            }),
        )
        .await;
    let result = &init["result"];
    assert_eq!(result["protocolVersion"], PROTOCOL_VERSION, "{init}");
    assert_eq!(result["serverInfo"]["name"], "viamind");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(result["capabilities"]["tools"].is_object(), "{init}");
    assert!(
        result["instructions"]
            .as_str()
            .is_some_and(|s| s.contains("Viamind")),
        "{init}"
    );

    client
        .send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await;

    // tools/list
    let list = client.request(2, "tools/list", json!({})).await;
    let tools = list["result"]["tools"].as_array().expect("tools array");
    let mut names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(names, ["echo", "viamind_version"], "{list}");
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()));
    }

    // tools/call echo
    let args = json!({ "text": "hello", "nested": { "n": 3, "list": [1, 2] } });
    let call = client
        .request(
            3,
            "tools/call",
            json!({ "name": "echo", "arguments": args }),
        )
        .await;
    let result = &call["result"];
    assert_eq!(result["isError"], false, "{call}");
    assert_eq!(result["structuredContent"], args, "{call}");
    let text = result["content"][0]["text"].as_str().expect("text content");
    assert_eq!(serde_json::from_str::<Value>(text).unwrap(), args);

    // tools/call viamind_version
    let call = client
        .request(
            4,
            "tools/call",
            json!({ "name": "viamind_version", "arguments": {} }),
        )
        .await;
    assert_eq!(
        call["result"]["structuredContent"],
        json!({ "name": "viamind", "version": env!("CARGO_PKG_VERSION") }),
        "{call}"
    );

    // Unknown tools are a protocol error.
    let call = client
        .request(5, "tools/call", json!({ "name": "nope", "arguments": {} }))
        .await;
    assert!(call["error"].is_object(), "{call}");

    // Closing stdin ends the session and the process exits cleanly.
    drop(client);
    let status = timeout(TIMEOUT, child.wait()).await.unwrap().unwrap();
    assert!(status.success(), "server exited with {status}");
}
