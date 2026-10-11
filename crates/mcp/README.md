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

## Shared desktop selection

The live desktop engine also exposes `get_selection` and `set_selection`.
`get_selection` takes `{}` and returns `ids`, `revision`, and the last `origin`.
`set_selection` takes `{"ids":["<board part UUID>"],"mode":"replace"}`;
`mode` defaults to `replace`, and can also be `add`, `remove`, or `toggle`.
An empty replacement clears selection. Unknown IDs reject the entire request.
These session actions wake the desktop and preserve the document undo/redo stack.
Deleting a selected part or undoing its creation removes it from the selection.

## Verify a running desktop

The desktop serves streamable HTTP at `http://127.0.0.1:7878/mcp` by default.
Set `VIAMIND_MCP_PORT` when launching the desktop to use another port. Connect an
agent client to that URL; the standalone `viamind-mcp` binary above serves only
identification/echo tools, so use the desktop endpoint to edit the open design.

From the repository root, with the desktop running:

```sh
python3 scripts/mcp-smoke.py --url http://127.0.0.1:7878/mcp --report /tmp/viamind-mcp-smoke.json
```

This standard-library client performs the MCP handshake, discovers tools, and
uses two distinct editing agent sessions to place, move, select, delete, undo, and redo
temporary parts. It checks useful validation errors, exact nanometre positions,
selective undo isolation, and conflicts on overlapping edits. It returns a JSON
report and exits nonzero on failure. `--observe-seconds 15` pauses with the test
parts selected so you can see their appearance in the desktop.

Run while the design is otherwise idle: the test compares the full document
before and after. Cleanup deletes only the parts it created and restores the
previous selection if another user or agent has not changed it. The document
returns to its initial state; the edit audit history retains the test operations.
An observer also checks atomic snapshots, event attribution, reconnect cursors, wait timeouts,
and selection pruning order. The HTTP client handles JSON responses and SSE responses, including the empty
priming event and heartbeat frames permitted by the transport.

## Query board parts

`query_parts` reads the live board without editing it. Combine filters with AND:

```json
{
  "text": "10k",
  "side": "top",
  "bounds": {"min_x_mm": 0, "min_y_mm": 0, "max_x_mm": 50, "max_y_mm": 50},
  "limit": 100
}
```

Use `designator`, `value`, or `footprint` for exact case-sensitive matches, `text`
for a case-insensitive substring across those fields, and `ids` for known UUIDs.
Bounds include their edges and test the part's origin. The response contains
`parts` (full part records with integer nanometre positions), `total` (all
matches), and `next_after` (null when done). Results are in UUID order. For the
next page, repeat the same filters and pass `next_after` as `after`. `limit`
defaults to 100, maximum 1000. Restart pagination after design edits.
See [the complete query contract](../../docs/specs/part-queries.md).

## Watch design and selection changes

Call `get_session_state {}` for an atomic `project`, `selection` and `cursor` snapshot.
Then call `watch_events` with that cursor:

```json
{"after": {"stream_id": "<session UUID>", "sequence": 0}, "limit": 100, "timeout_ms": 30000}
```

The result contains ordered `events`, the next `cursor`, and `has_more`. Records have a
`sequence` and an `event` with `kind` (`design` or `selection`) and `data`. Design data is
the transaction change (including deltas and origin); selection data is the complete shared
selection. Apply events in order and retain the returned cursor. Drain remaining pages when
`has_more` is true, then wait again. Both stdio and HTTP support this tool.

`limit` accepts 1–1000 (default 100); `timeout_ms` accepts 0–30000 (default 0, immediate).
Without `after`, watching starts at the current tail. On timeout the page is empty and its
cursor is unchanged. Use one pending watch per observer; a cancelled call may occupy a worker
until its timeout. Cursors survive client reconnects to the same engine. They expire when
more than 1024 newer records arrive, or when the engine restarts. On a cursor error, call
`get_session_state` to resynchronize. Reading events preserves undo ownership and history.
See [the full session event contract](../../docs/specs/session-events.md).
