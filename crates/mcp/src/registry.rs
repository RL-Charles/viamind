//! Transport-independent tool registry.
//!
//! A tool is a name, a description, a JSON Schema for its input and an async
//! handler from JSON input to JSON output. The MCP server lists and calls
//! whatever is registered here, so engine commands (T-0024) only need to be
//! turned into [`ToolDef`]s and handlers; nothing in this module knows about MCP.

use std::{collections::BTreeMap, fmt, future::Future, pin::Pin, sync::Arc};

use serde_json::Value;

use crate::agent::CallContext;

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

/// Type-erased async tool handler: JSON input (plus who is calling) to JSON output.
pub type ToolHandler = Arc<dyn Fn(Value, CallContext) -> ToolFuture + Send + Sync>;

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
    png_output: bool,
    observation: bool,
    in_memory_def: Option<ToolDef>,
}

/// Set of tools served by the MCP server, ordered by name.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Entry>,
    read_only: bool,
}

impl fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.tools().map(|tool| &tool.name)).finish()
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

    /// Restrict this registry and its future registrations to trusted observations.
    /// Clones retain the restriction; there is no client-controlled admission metadata.
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Whether agent calls are restricted to observations.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Register a trusted observation. General registration always defaults to mutating.
    pub fn register_observation<F, Fut>(
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
        let name = name.into();
        self.register(name.clone(), description, input_schema, handler)?;
        self.admit_observation(&name);
        Ok(())
    }

    /// Trusted host registration only, immediately after registering the audited handler.
    pub(crate) fn admit_observation(&mut self, name: &str) {
        self.tools.get_mut(name).expect("classify an existing trusted registration").observation = true;
    }

    /// Register a trusted SVG/PNG view with optional top-level file output.
    /// Restricted calls omit `path` entirely, and reject it before invoking the handler.
    pub fn register_in_memory_view<F, Fut>(
        &mut self,
        name: &str,
        description: &str,
        schema: Value,
        png_output: bool,
        handler: F,
    ) -> Result<(), RegistryError>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        let mut in_memory_schema = schema.clone();
        let properties =
            in_memory_schema.get_mut("properties").and_then(Value::as_object_mut).ok_or_else(|| RegistryError::InvalidSchema(name.into()))?;
        if properties.remove("path").is_none() {
            return Err(RegistryError::InvalidSchema(name.into()));
        }
        if let Some(required) = in_memory_schema.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|field| field != "path");
        }
        in_memory_schema["additionalProperties"] = Value::Bool(false);
        self.register_observation(name, description, schema, handler)?;
        let entry = self.tools.get_mut(name).expect("just registered");
        entry.png_output = png_output;
        entry.in_memory_def = Some(ToolDef {
            name: name.into(),
            description: format!("{description} Agents in this process are read only: omit path; file output is unavailable."),
            input_schema: in_memory_schema,
        });
        Ok(())
    }

    /// Register a tool with an async handler that does not need to know who is calling.
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
        self.register_with_context(name, description, input_schema, move |input, _ctx| handler(input))
    }

    /// Register a tool whose async handler also receives the [`CallContext`]
    /// (e.g. the calling [`crate::AgentId`]).
    pub fn register_with_context<F, Fut>(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: F,
    ) -> Result<(), RegistryError>
    where
        F: Fn(Value, CallContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        let def = ToolDef { name: name.into(), description: description.into(), input_schema };
        let handler: ToolHandler = Arc::new(move |input, ctx| Box::pin(handler(input, ctx)) as ToolFuture);
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
        self.tools.insert(def.name.clone(), Entry { def, handler, png_output: false, observation: false, in_memory_def: None });
        Ok(())
    }

    /// Register a trusted PNG handler. Its `png_base64` field becomes an MCP image
    /// block and is omitted from the structured/text metadata on the wire.
    pub fn register_png<F, Fut>(&mut self, name: &str, description: &str, schema: Value, handler: F) -> Result<(), RegistryError>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        self.register(name, description, schema, handler)?;
        self.tools.get_mut(name).expect("just registered").png_output = true;
        Ok(())
    }

    pub(crate) fn png_output(&self, name: &str) -> bool {
        self.tools.get(name).is_some_and(|entry| self.definition(entry).is_some() && entry.png_output)
    }

    fn definition<'a>(&self, entry: &'a Entry) -> Option<&'a ToolDef> {
        if !self.read_only {
            Some(&entry.def)
        } else if entry.observation {
            Some(entry.in_memory_def.as_ref().unwrap_or(&entry.def))
        } else {
            None
        }
    }

    /// Definition of the tool called `name`.
    pub fn get(&self, name: &str) -> Option<&ToolDef> {
        self.tools.get(name).and_then(|entry| self.definition(entry))
    }

    /// All tool definitions, ordered by name.
    pub fn tools(&self) -> impl Iterator<Item = &ToolDef> {
        self.tools.values().filter_map(|entry| self.definition(entry))
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools().count()
    }

    /// Whether no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Call the tool called `name` with `input` on behalf of `ctx.agent`.
    pub async fn call(&self, name: &str, input: Value, ctx: CallContext) -> Result<Value, CallError> {
        let entry = self.tools.get(name).filter(|entry| self.definition(entry).is_some()).ok_or_else(|| CallError::UnknownTool(name.to_owned()))?;
        if self.read_only && entry.in_memory_def.is_some() && (!input.is_object() || input.get("path").is_some()) {
            return Err(ToolError::InvalidInput("agents are read only; omit path for an in-memory view".into()).into());
        }
        let handler = entry.handler.clone();
        Ok(handler(input, ctx).await?)
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

    #[test]
    fn only_explicit_trusted_registration_selects_image_output() {
        let mut registry = ToolRegistry::new();
        registry.register("ordinary", "plain", object_schema(), |_| async { Ok(json!({"png_base64":"AA=="})) }).unwrap();
        registry.register_png("picture", "image", object_schema(), |_| async { Ok(json!({"png_base64":"AA=="})) }).unwrap();
        assert!(!registry.png_output("ordinary"));
        assert!(registry.clone().png_output("picture"));
        assert!(!registry.png_output("unknown"));
        assert!(registry.register_png("ordinary", "duplicate", object_schema(), |_| async { Ok(Value::Null) }).is_err());
        assert!(!registry.png_output("ordinary"));
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

        let out = registry.call("double", json!({ "n": 21 }), CallContext::default()).await.unwrap();
        assert_eq!(out, json!({ "n": 42 }));
    }

    #[tokio::test]
    async fn handler_errors_and_unknown_tools() {
        let mut registry = ToolRegistry::new();
        registry.register("fail", "Always fails", object_schema(), |_| async { Err(ToolError::Failed("boom".into())) }).unwrap();

        assert_eq!(registry.call("fail", json!({}), CallContext::default()).await, Err(CallError::Tool(ToolError::Failed("boom".into()))));
        assert_eq!(registry.call("missing", json!({}), CallContext::default()).await, Err(CallError::UnknownTool("missing".into())));
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
    async fn context_reaches_the_handler() {
        let mut registry = ToolRegistry::new();
        registry.register_with_context("whoami", "", object_schema(), |_, ctx| async move { Ok(json!(ctx.agent.0)) }).unwrap();
        assert_eq!(registry.call("whoami", json!({}), CallContext::new("bob#1")).await.unwrap(), json!("bob#1"));
        assert_eq!(registry.call("whoami", json!({}), CallContext::default()).await.unwrap(), json!(crate::AgentId::LOCAL));
    }

    #[tokio::test]
    async fn builtins_are_registered() {
        let registry = ToolRegistry::with_builtins();
        let names: Vec<_> = registry.tools().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["echo", "viamind_version"]);

        let input = json!({ "hello": [1, 2, 3] });
        assert_eq!(registry.call("echo", input.clone(), CallContext::default()).await.unwrap(), input);

        let version = registry.call("viamind_version", json!({}), CallContext::default()).await.unwrap();
        assert_eq!(version["name"], "viamind");
        assert_eq!(version["version"], env!("CARGO_PKG_VERSION"));
    }
}
