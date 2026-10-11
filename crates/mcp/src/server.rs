//! rmcp server that lists and calls tools from a [`ToolRegistry`].
//!
//! One [`ViamindServer`] serves one MCP connection. [`serve`] runs it on any
//! rmcp transport (stdio, an in-process `tokio::io::duplex` pipe, a
//! `(Sink, Stream)` channel pair, ...); [`serve_stdio`] is the stdio shortcut
//! the `viamind-mcp` binary uses. Many connections can share one registry
//! (and so one engine) through an `Arc<ToolRegistry>`.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::{RequestContext, RunningService, ServerInitializeError},
    transport::IntoTransport,
};
use serde_json::Value;

use crate::{
    INSTRUCTIONS, SERVER_NAME,
    agent::{AgentId, CallContext},
    registry::{CallError, ToolDef, ToolRegistry},
};

/// A running MCP connection: await [`RunningService::waiting`] for it to end,
/// or call [`RunningService::cancel`] to close it.
pub type RunningServer = RunningService<RoleServer, ViamindServer>;

/// Agent name used when the client did not send `clientInfo.name`.
pub const UNKNOWN_AGENT: &str = "unknown-agent";

/// Source of per-connection numbers; see [`ViamindServer::connection`].
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

/// MCP server handler for one connection, backed by a shared [`ToolRegistry`].
///
/// Each instance gets a process-unique connection number at construction, so
/// build a new one per connection (it is deliberately not `Clone`). Tool calls
/// run as the [`AgentId`] `<clientInfo.name>#<connection>`.
#[derive(Debug)]
pub struct ViamindServer {
    registry: Arc<ToolRegistry>,
    connection: u64,
}

impl ViamindServer {
    /// Server exposing the tools in `registry`; pass an `Arc<ToolRegistry>` to
    /// share one registry between connections.
    pub fn new(registry: impl Into<Arc<ToolRegistry>>) -> Self {
        Self { registry: registry.into(), connection: NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed) }
    }

    /// The registry this server serves.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// This connection's number, unique within the process.
    pub fn connection(&self) -> u64 {
        self.connection
    }

    /// The agent id for a caller on this connection that reported
    /// `client_name` (missing or blank: [`UNKNOWN_AGENT`]).
    pub fn agent_id(&self, client_name: Option<&str>) -> AgentId {
        let name = client_name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or(UNKNOWN_AGENT);
        AgentId(format!("{name}#{}", self.connection))
    }
}

/// Convert a registry definition to an MCP tool.
fn to_mcp_tool(def: &ToolDef) -> Tool {
    let schema = match &def.input_schema {
        Value::Object(map) => map.clone(),
        // The registry only accepts object schemas.
        _ => unreachable!("registry rejects non-object schemas"),
    };
    Tool::new(def.name.clone(), def.description.clone(), schema)
}

/// Wrap a tool's JSON output as an MCP result. Objects become structured
/// content (plus a text copy for older clients); other values are text only,
/// since MCP structured content must be an object.
fn to_call_result(value: Value) -> CallToolResult {
    if value.is_object() { CallToolResult::structured(value) } else { CallToolResult::success(vec![ContentBlock::text(value.to_string())]) }
}

impl ServerHandler for ViamindServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"))
                    .with_title("Viamind")
                    .with_website_url("https://github.com/RL-Charles/viamind"),
            )
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.registry.tools().map(to_mcp_tool).collect()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.registry.get(name).map(to_mcp_tool)
    }

    async fn call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let input = Value::Object(request.arguments.unwrap_or_default());
        // Covers both the initialize handshake and per-request `_meta` client
        // info (the stateless protocol revisions).
        let client = context.client_info();
        let ctx = CallContext::new(self.agent_id(client.as_ref().map(|info| info.name.as_str())));
        match self.registry.call(&request.name, input, ctx).await {
            Ok(value) => Ok(to_call_result(value).into()),
            // Tool failures go back as tool results so the agent sees the message.
            Err(CallError::Tool(err)) => Ok(CallToolResult::error(vec![ContentBlock::text(err.to_string())]).into()),
            Err(err @ CallError::UnknownTool(_)) => Err(McpError::invalid_params(err.to_string(), None)),
        }
    }
}

/// Serve `registry` as one MCP connection over `transport`.
///
/// Resolves once the client has finished the MCP handshake, with the running
/// connection. `transport` is anything rmcp accepts, for example:
///
/// - `rmcp::transport::stdio()` (see [`serve_stdio`]);
/// - one end of `tokio::io::duplex(..)`, or any `AsyncRead + AsyncWrite`, for
///   an in-process client;
/// - a `(Sink, Stream)` pair of rmcp messages, e.g. over channels.
///
/// For many clients, call it once per connection with clones of one
/// `Arc<ToolRegistry>`; each connection gets its own [`AgentId`].
pub async fn serve<T, E, A>(registry: impl Into<Arc<ToolRegistry>>, transport: T) -> Result<RunningServer, Box<ServerInitializeError>>
where
    T: IntoTransport<RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    ViamindServer::new(registry).serve(transport).await.map_err(Box::new)
}

/// Serve `registry` over stdin/stdout until the client disconnects.
///
/// Stdout carries the protocol, so nothing else may write to it.
pub async fn serve_stdio(registry: impl Into<Arc<ToolRegistry>>) -> anyhow::Result<()> {
    serve(registry, rmcp::transport::stdio()).await?.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn objects_are_structured_other_values_are_text() {
        let result = to_call_result(json!({ "a": 1 }));
        assert_eq!(result.structured_content, Some(json!({ "a": 1 })));
        assert_eq!(result.is_error, Some(false));

        let result = to_call_result(json!([1, 2]));
        assert_eq!(result.structured_content, None);
        assert_eq!(result.content[0].as_text().unwrap().text, "[1,2]");
    }

    #[test]
    fn info_and_tools_come_from_the_registry() {
        let server = ViamindServer::new(ToolRegistry::with_builtins());
        let info = server.get_info();
        assert_eq!(info.server_info.name, SERVER_NAME);
        assert!(info.capabilities.tools.is_some());
        let echo = server.get_tool("echo").unwrap();
        assert_eq!(echo.input_schema.get("type"), Some(&json!("object")));
        assert!(server.get_tool("missing").is_none());
    }

    #[test]
    fn agent_ids_combine_client_name_and_connection() {
        let registry = Arc::new(ToolRegistry::new());
        let a = ViamindServer::new(registry.clone());
        let b = ViamindServer::new(registry);
        assert_ne!(a.connection(), b.connection());
        assert_eq!(a.agent_id(Some("claude-code")).0, format!("claude-code#{}", a.connection()));
        assert_eq!(b.agent_id(Some(" ")).0, format!("{UNKNOWN_AGENT}#{}", b.connection()));
        assert_eq!(b.agent_id(None).0, format!("{UNKNOWN_AGENT}#{}", b.connection()));
    }
}
