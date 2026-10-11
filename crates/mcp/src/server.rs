//! rmcp server that lists and calls tools from a [`ToolRegistry`].

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use serde_json::Value;

use crate::{
    INSTRUCTIONS, SERVER_NAME,
    registry::{CallError, ToolDef, ToolRegistry},
};

/// MCP server handler backed by a [`ToolRegistry`].
///
/// Serve it on any rmcp transport with [`rmcp::ServiceExt::serve`], or use
/// [`serve_stdio`].
#[derive(Debug, Clone)]
pub struct ViamindServer {
    registry: Arc<ToolRegistry>,
}

impl ViamindServer {
    /// Server exposing the tools in `registry`.
    pub fn new(registry: ToolRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    /// The registry this server serves.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
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
    if value.is_object() {
        CallToolResult::structured(value)
    } else {
        CallToolResult::success(vec![ContentBlock::text(value.to_string())])
    }
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

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(
            self.registry.tools().map(to_mcp_tool).collect(),
        ))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.registry.get(name).map(to_mcp_tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let input = Value::Object(request.arguments.unwrap_or_default());
        match self.registry.call(&request.name, input).await {
            Ok(value) => Ok(to_call_result(value).into()),
            // Tool failures go back as tool results so the agent sees the message.
            Err(CallError::Tool(err)) => {
                Ok(CallToolResult::error(vec![ContentBlock::text(err.to_string())]).into())
            }
            Err(err @ CallError::UnknownTool(_)) => {
                Err(McpError::invalid_params(err.to_string(), None))
            }
        }
    }
}

/// Serve `registry` over stdin/stdout until the client disconnects.
///
/// Stdout carries the protocol, so nothing else may write to it.
pub async fn serve_stdio(registry: ToolRegistry) -> anyhow::Result<()> {
    let service = ViamindServer::new(registry)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
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
}
