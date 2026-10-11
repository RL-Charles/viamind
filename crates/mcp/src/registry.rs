//! Transport-independent tool registry.
//!
//! A tool is a name, a description, a JSON Schema for its input and an async
//! handler from JSON input to JSON output. The MCP server lists and calls
//! whatever is registered here, so engine commands (T-0024) only need to be
//! turned into [`ToolDef`]s and handlers; nothing in this module knows about MCP.

use std::{collections::BTreeMap, fmt, future::Future, pin::Pin, sync::Arc};

use serde_json::Value;

/// Error returned by a tool handler. The message is shown to the calling agent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// The input did not match what the tool expects.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The tool ran and failed.
    #[error("{0}")]
    Failed(String),
}

/// Error from [`ToolRegistry::register`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// A tool with this name is already registered.
    #[error("tool `{0}` is already registered")]
    Duplicate(String),
    /// The tool name is empty or uses characters MCP clients reject.
    #[error("invalid tool name `{0}`: use 1-128 characters from [A-Za-z0-9_.-]")]
    InvalidName(String),
    /// The input schema is not a JSON object.
    #[error("input schema for `{0}` must be a JSON object")]
    InvalidSchema(String),
}

/// Error from [`ToolRegistry::call`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    /// No tool with this name is registered.
    #[error("unknown tool `{0}`")]
    UnknownTool(String),
    /// The tool's handler returned an error.
    #[error(transparent)]
    Tool(#[from] ToolError),
}

/// Boxed future returned by a [`ToolHandler`].
pub type ToolFuture = Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send>>;

/// Type-erased async tool handler: JSON input to JSON output.
pub type ToolHandler = Arc<dyn Fn(Value) -> ToolFuture + Send + Sync>;

/// Static description of a tool, as advertised to clients.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDef {
    /// Unique tool name, e.g. `viamind_version`.
    pub name: String,
    /// Human- and agent-readable description.
    pub description: String,
    /// JSON Schema (an object) describing the tool's input.
    pub input_schema: Value,
}

#[derive(Clone)]
struct Entry {
    def: ToolDef,
    handler: ToolHandler,
}

/// Set of tools served by the MCP server, ordered by name.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Entry>,
}

impl fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.tools.keys()).finish()
    }
}

impl ToolRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry pre-filled with the built-in tools (see [`crate::builtin`]).
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        crate::builtin::register_builtins(&mut registry).expect("built-in tool definitions are valid and unique");
        registry
    }

    /// Register a tool with an async handler.
    pub fn register<F, Fut>(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: F,
    ) -> Result<(), RegistryError>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        let def = ToolDef { name: name.into(), description: description.into(), input_schema };
        let handler: ToolHandler = Arc::new(move |input| Box::pin(handler(input)) as ToolFuture);
        self.register_def(def, handler)
    }

    /// Register a tool from an already type-erased definition and handler.
    pub fn register_def(&mut self, def: ToolDef, handler: ToolHandler) -> Result<(), RegistryError> {
        if !is_valid_name(&def.name) {
            return Err(RegistryError::InvalidName(def.name));
        }
        if !def.input_schema.is_object() {
            return Err(RegistryError::InvalidSchema(def.name));
        }
        if self.tools.contains_key(&def.name) {
            return Err(RegistryError::Duplicate(def.name));
        }
        self.tools.insert(def.name.clone(), Entry { def, handler });
        Ok(())
    }

    /// Definition of the tool called `name`.
    pub fn get(&self, name: &str) -> Option<&ToolDef> {
        self.tools.get(name).map(|entry| &entry.def)
    }

    /// All tool definitions, ordered by name.
    pub fn tools(&self) -> impl Iterator<Item = &ToolDef> {
        self.tools.values().map(|entry| &entry.def)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Call the tool called `name` with `input`.
    pub async fn call(&self, name: &str, input: Value) -> Result<Value, CallError> {
        let handler = self.tools.get(name).map(|entry| entry.handler.clone()).ok_or_else(|| CallError::UnknownTool(name.to_owned()))?;
        Ok(handler(input).await?)
    }
}

/// Tool names per the MCP spec: 1-128 characters from `[A-Za-z0-9_.-]`.
fn is_valid_name(name: &str) -> bool {
    (1..=128).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn object_schema() -> Value {
        json!({ "type": "object" })
    }

    #[tokio::test]
    async fn registers_lists_and_calls() {
        let mut registry = ToolRegistry::new();
        registry
            .register("double", "Doubles n", object_schema(), |input| async move {
                let n = input["n"].as_i64().ok_or_else(|| ToolError::InvalidInput("n must be an integer".into()))?;
                Ok(json!({ "n": n * 2 }))
            })
            .unwrap();
        registry.register("a_first", "Sorts first", object_schema(), |_| async { Ok(Value::Null) }).unwrap();

        assert_eq!(registry.len(), 2);
        let names: Vec<_> = registry.tools().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a_first", "double"]);
        assert_eq!(registry.get("double").unwrap().description, "Doubles n");

        let out = registry.call("double", json!({ "n": 21 })).await.unwrap();
        assert_eq!(out, json!({ "n": 42 }));
    }

    #[tokio::test]
    async fn handler_errors_and_unknown_tools() {
        let mut registry = ToolRegistry::new();
        registry.register("fail", "Always fails", object_schema(), |_| async { Err(ToolError::Failed("boom".into())) }).unwrap();

        assert_eq!(registry.call("fail", json!({})).await, Err(CallError::Tool(ToolError::Failed("boom".into()))));
        assert_eq!(registry.call("missing", json!({})).await, Err(CallError::UnknownTool("missing".into())));
    }

    #[test]
    fn rejects_duplicates_bad_names_and_bad_schemas() {
        let noop = |_| async { Ok(Value::Null) };
        let mut registry = ToolRegistry::new();
        registry.register("t", "", object_schema(), noop).unwrap();
        assert_eq!(registry.register("t", "", object_schema(), noop), Err(RegistryError::Duplicate("t".into())));
        assert_eq!(registry.register("", "", object_schema(), noop), Err(RegistryError::InvalidName(String::new())));
        assert_eq!(registry.register("has space", "", object_schema(), noop), Err(RegistryError::InvalidName("has space".into())));
        assert_eq!(registry.register("s", "", json!(true), noop), Err(RegistryError::InvalidSchema("s".into())));
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn builtins_are_registered() {
        let registry = ToolRegistry::with_builtins();
        let names: Vec<_> = registry.tools().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["echo", "viamind_version"]);

        let input = json!({ "hello": [1, 2, 3] });
        assert_eq!(registry.call("echo", input.clone()).await.unwrap(), input);

        let version = registry.call("viamind_version", json!({})).await.unwrap();
        assert_eq!(version["name"], "viamind");
        assert_eq!(version["version"], env!("CARGO_PKG_VERSION"));
    }
}
