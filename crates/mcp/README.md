# viamind-mcp

The Viamind agent harness: an [MCP](https://modelcontextprotocol.io) server, built on the
official Rust SDK [`rmcp`](https://crates.io/crates/rmcp), that exposes Viamind as tools.

```
MCP client ──transport──▶ ViamindServer (one per connection) ──▶ ToolRegistry ──▶ handlers
  clientInfo.name ─────▶ AgentId "name#conn" ──────────────────────────┘           │
                                                                                     ▼
                                     CommandHost (the engine): execute / undo / redo / snapshot
```

Built-in tools: `viamind_version` and `echo`. With a `CommandHost` registered, every engine
command is a tool too, plus `undo`, `redo` and `get_design`. The `viamind-mcp` binary has no
engine yet, so it serves the built-ins only.

## Use it from Claude Code

```sh
cargo build --release -p viamind-mcp
claude mcp add viamind -- "$PWD/target/release/viamind-mcp"
```

Any other MCP client works the same way: run `viamind-mcp` with no arguments and speak MCP on
its stdin/stdout.

## Expose engine commands

The engine implements `CommandHost`; `viamind-mcp` does not depend on its concrete types.

```rust
pub struct HostCommand { pub name: String, pub description: String, pub input_schema: serde_json::Value }

pub trait CommandHost: Send + Sync + 'static {
    fn commands(&self) -> Vec<HostCommand>;
    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String>;
    fn undo(&self, agent: &AgentId) -> Result<Value, String>;
    fn redo(&self, agent: &AgentId) -> Result<Value, String>;
    fn snapshot(&self) -> Value;
}

let mut registry = ToolRegistry::with_builtins();
register_host(&mut registry, engine)?; // engine: Arc<dyn CommandHost>
serve_stdio(registry).await?;
```

`register_host` adds one tool per command (same name, description and input schema) and the
built-ins `undo`, `redo` (the calling agent's own history) and `get_design` (`snapshot()`).
Registration is all or nothing: a duplicate or invalid name leaves the registry unchanged. Host
methods may block; they run on tokio's blocking pool. An `Err(message)` from the host comes
back to the agent as a tool result with `isError: true` and that message.

## Agent identity

Each connection gets a process-unique number when its `ViamindServer` is created. A tool call
runs as `AgentId("<clientInfo.name>#<number>")`, e.g. `claude-code#3`; the name comes from the
`initialize` handshake or the request's `_meta` client info, and is `unknown-agent` if absent.
Two sessions of the same client are therefore different agents. Handlers that need it register
with `ToolRegistry::register_with_context` and receive a `CallContext { agent }`.

## Transports

`serve(registry, transport)` runs one connection on any rmcp transport and returns the
`RunningServer` once the handshake is done; `serve_stdio(registry)` is the stdio shortcut.
`registry` is a `ToolRegistry` or an `Arc<ToolRegistry>`; pass clones of one `Arc` to serve
many connections against the same engine. For an in-process client (as the UI does in T-0026):

```rust
let (server_end, client_end) = tokio::io::duplex(64 * 1024);
let server = viamind_mcp::serve(registry.clone(), server_end).await?; // hand client_end to the client
```

`(Sink, Stream)` channel pairs and any `AsyncRead + AsyncWrite` work the same way.

## Add a plain tool

```rust
registry.register(
    "add",
    "Add two integers.",
    json!({ "type": "object", "properties": { "a": { "type": "integer" }, "b": { "type": "integer" } } }),
    |input| async move {
        let (Some(a), Some(b)) = (input["a"].as_i64(), input["b"].as_i64()) else {
            return Err(ToolError::InvalidInput("a and b must be integers".into()));
        };
        Ok(json!({ "sum": a + b }))
    },
)?;
```

Object results are returned as `structuredContent` plus a JSON text copy; a `ToolError` is
returned as a tool result with `isError: true` so the agent sees the message.

## Test

`cargo test -p viamind-mcp` runs the unit tests, `tests/stdio.rs` (spawns the binary and drives
raw JSON-RPC over stdio) and `tests/host.rs` (a fake in-memory `CommandHost` served over
`tokio::io::duplex` to real rmcp clients: tools/list, tools/call, undo/redo, get_design, host
errors and per-connection agent ids).
