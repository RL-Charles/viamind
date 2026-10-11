//! Engine commands as MCP tools.
//!
//! The engine (lane A) implements [`CommandHost`]; [`register_host`] turns each
//! of its commands into a tool with the same name, description and JSON input
//! schema, and adds the agent-facing built-ins `undo`, `redo` and `get_design`.
//! Keeping the engine behind this trait means `viamind-mcp` does not depend on
//! the engine's concrete types.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    agent::{AgentId, CallContext},
    registry::{RegistryError, ToolError, ToolRegistry},
};

/// Name of the built-in undo tool.
pub const UNDO_TOOL: &str = "undo";
/// Name of the built-in redo tool.
pub const REDO_TOOL: &str = "redo";
/// Name of the built-in read-only design snapshot tool.
pub const GET_DESIGN_TOOL: &str = "get_design";

const UNDO_DESCRIPTION: &str = "Undo the most recent design change you made on this connection. Changes made by \
the human in the Viamind UI or by other agents are not affected. Takes no arguments and returns a description of \
what was undone; fails with a message if you have nothing left to undo. Call it repeatedly to step further back.";

const REDO_DESCRIPTION: &str = "Redo the change you most recently undid with `undo` on this connection. Takes no \
arguments and returns a description of what was redone; fails with a message if there is nothing to redo (making \
a new change clears what can be redone).";

const GET_DESIGN_DESCRIPTION: &str = "Return the whole current design (project, schematic, board) as JSON. \
Read-only: it never changes anything. Call it to look at the design before you edit it, to find the ids that other \
tools take as arguments, and after editing to check the result, since the human may have changed things too.";

/// A command the host can execute, advertised to agents as one MCP tool.
#[derive(Debug, Clone, PartialEq)]
pub struct HostCommand {
    /// Tool name, 1-128 characters from `[A-Za-z0-9_.-]`, e.g. `place_component`.
    pub name: String,
    /// What the command does, written for an AI agent.
    pub description: String,
    /// JSON Schema (an object) for the command's input.
    pub input_schema: Value,
}

/// The design engine as seen by the agent harness.
///
/// Every method is synchronous and may block; the harness calls it from a
/// blocking-safe thread ([`tokio::task::spawn_blocking`]), never directly on
/// the async runtime. Errors are plain messages and are shown to the agent
/// verbatim, so make them actionable.
pub trait CommandHost: Send + Sync + 'static {
    /// The commands to expose as tools. Read once, by [`register_host`].
    fn commands(&self) -> Vec<HostCommand>;
    /// Trusted admission classification. New host commands default to mutating.
    fn is_observation(&self, _name: &str) -> bool {
        false
    }
    /// Run command `name` with `input` on behalf of `agent` and return its
    /// result. Document changes must be undoable and attributed to `agent`.
    /// Session actions such as selection preserve document history.
    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String>;
    /// Undo `agent`'s most recent change.
    fn undo(&self, agent: &AgentId) -> Result<Value, String>;
    /// Redo `agent`'s most recently undone change.
    fn redo(&self, agent: &AgentId) -> Result<Value, String>;
    /// The whole design as JSON, for read-only queries.
    fn snapshot(&self) -> Value;
}

/// Register one tool per [`CommandHost::commands`] entry plus the built-ins
/// [`UNDO_TOOL`], [`REDO_TOOL`] and [`GET_DESIGN_TOOL`].
///
/// All or nothing: if any tool is rejected (bad name or schema, or a name that
/// is already registered, including a host command called `undo`), `registry`
/// is left unchanged and the error is returned.
pub fn register_host(registry: &mut ToolRegistry, host: Arc<dyn CommandHost>) -> Result<(), RegistryError> {
    let mut staged = registry.clone();

    for command in host.commands() {
        let observation = host.is_observation(&command.name);
        let host = host.clone();
        let name = command.name.clone();
        let registered_name = name.clone();
        staged.register_with_context(command.name, command.description, command.input_schema, move |input, ctx: CallContext| {
            let host = host.clone();
            let name = name.clone();
            blocking(move || host.execute(&name, input, &ctx.agent))
        })?;
        if observation {
            staged.admit_observation(&registered_name);
        }
    }

    let no_args = || json!({ "type": "object", "properties": {}, "additionalProperties": false });
    let h = host.clone();
    staged.register_with_context(UNDO_TOOL, UNDO_DESCRIPTION, no_args(), move |_, ctx| {
        let host = h.clone();
        blocking(move || host.undo(&ctx.agent))
    })?;
    let h = host.clone();
    staged.register_with_context(REDO_TOOL, REDO_DESCRIPTION, no_args(), move |_, ctx| {
        let host = h.clone();
        blocking(move || host.redo(&ctx.agent))
    })?;
    staged.register_observation(GET_DESIGN_TOOL, GET_DESIGN_DESCRIPTION, no_args(), move |_| {
        let host = host.clone();
        blocking(move || Ok(host.snapshot()))
    })?;

    *registry = staged;
    Ok(())
}

/// Run a host call on tokio's blocking pool and map its error to a tool error.
async fn blocking<F>(f: F) -> Result<Value, ToolError>
where
    F: FnOnce() -> Result<Value, String> + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(result) => result.map_err(ToolError::Failed),
        Err(err) => Err(ToolError::Failed(format!("internal error: the command did not complete ({err})"))),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Counter host: `inc` adds to a shared counter; undo/redo are not supported.
    #[derive(Default)]
    struct Counter {
        value: Mutex<i64>,
        names: Vec<&'static str>,
    }

    impl CommandHost for Counter {
        fn commands(&self) -> Vec<HostCommand> {
            self.names
                .iter()
                .map(|name| HostCommand {
                    name: (*name).to_owned(),
                    description: format!("{name} the counter"),
                    input_schema: json!({ "type": "object", "properties": { "by": { "type": "integer" } } }),
                })
                .collect()
        }

        fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String> {
            let by = input["by"].as_i64().ok_or("`by` must be an integer")?;
            let mut value = self.value.lock().unwrap();
            *value += by;
            Ok(json!({ "command": name, "value": *value, "agent": agent.as_str() }))
        }

        fn undo(&self, _agent: &AgentId) -> Result<Value, String> {
            Err("nothing to undo".into())
        }

        fn redo(&self, _agent: &AgentId) -> Result<Value, String> {
            Err("nothing to redo".into())
        }

        fn snapshot(&self) -> Value {
            json!({ "value": *self.value.lock().unwrap() })
        }
    }

    fn counter(names: &[&'static str]) -> Arc<Counter> {
        Arc::new(Counter { names: names.to_vec(), ..Counter::default() })
    }

    #[tokio::test]
    async fn host_commands_become_tools() {
        let host = counter(&["inc"]);
        let mut registry = ToolRegistry::new();
        register_host(&mut registry, host.clone()).unwrap();

        let names: Vec<_> = registry.tools().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["get_design", "inc", "redo", "undo"]);
        assert_eq!(registry.get("inc").unwrap().input_schema, host.commands()[0].input_schema);
        assert_eq!(registry.get("inc").unwrap().description, "inc the counter");

        let out = registry.call("inc", json!({ "by": 2 }), CallContext::new("ann#1")).await.unwrap();
        assert_eq!(out, json!({ "command": "inc", "value": 2, "agent": "ann#1" }));
        assert_eq!(registry.call(GET_DESIGN_TOOL, json!({}), CallContext::default()).await.unwrap(), json!({ "value": 2 }));

        let err = registry.call("inc", json!({}), CallContext::default()).await.unwrap_err();
        assert_eq!(err.to_string(), "`by` must be an integer");
        let err = registry.call(UNDO_TOOL, json!({}), CallContext::default()).await.unwrap_err();
        assert_eq!(err.to_string(), "nothing to undo");
    }

    #[test]
    fn registration_is_all_or_nothing() {
        let mut registry = ToolRegistry::with_builtins();
        let before: Vec<_> = registry.tools().cloned().collect();

        // `undo` clashes with the built-in added after the host commands.
        let err = register_host(&mut registry, counter(&["inc", "undo"])).unwrap_err();
        assert_eq!(err, RegistryError::Duplicate("undo".into()));
        assert_eq!(registry.tools().cloned().collect::<Vec<_>>(), before);

        let err = register_host(&mut registry, counter(&["echo"])).unwrap_err();
        assert_eq!(err, RegistryError::Duplicate("echo".into()));
        assert_eq!(registry.len(), before.len());
    }
}
