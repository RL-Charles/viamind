//! `viamind-mcp`: the Viamind MCP server on stdio.
//!
//! Register it with an MCP client, e.g. `claude mcp add viamind -- /path/to/viamind-mcp`.

use viamind_mcp::{ToolRegistry, serve_stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    serve_stdio(ToolRegistry::with_builtins()).await
}
