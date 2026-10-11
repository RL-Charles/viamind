# viamind-mcp

The Viamind agent harness: an [MCP](https://modelcontextprotocol.io) server, built on the
official Rust SDK [`rmcp`](https://crates.io/crates/rmcp), that exposes Viamind as tools.

```
MCP client ──stdio JSON-RPC──▶ viamind-mcp ──▶ ViamindServer ──▶ ToolRegistry ──▶ handlers
```

Tools available today: `viamind_version` and `echo`. Engine commands are added as tools in T-0024.

## Use it from Claude Code

```sh
cargo build --release -p viamind-mcp
claude mcp add viamind -- "$PWD/target/release/viamind-mcp"
```

Any other MCP client works the same way: run `viamind-mcp` with no arguments and speak MCP on
its stdin/stdout.

## Add a tool

```rust
use serde_json::json;
use viamind_mcp::{ToolError, ToolRegistry, serve_stdio};

let mut registry = ToolRegistry::with_builtins();
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
serve_stdio(registry).await?;
```

Object results are returned as `structuredContent` plus a JSON text copy; a `ToolError` is
returned as a tool result with `isError: true` so the agent sees the message.

## Test

`cargo test -p viamind-mcp` runs the registry unit tests and `tests/stdio.rs`, which spawns the
binary and drives `initialize`, `tools/list` and `tools/call` over raw JSON-RPC.
