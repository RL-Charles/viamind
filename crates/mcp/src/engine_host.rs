//! The design engine as a [`CommandHost`], so every engine command is an MCP tool.

use serde_json::{Value, json};
use viamind_engine::{EngineHandle, Origin};

use crate::{
    agent::AgentId,
    host::{CommandHost, HostCommand},
};

/// Adapts an [`EngineHandle`] to the agent harness.
///
/// Agent edits are recorded with [`Origin::Agent`] so the UI can attribute them, and agents undo only their
/// own changes ([`EngineHandle::undo_own`]); the human can still undo anything from the UI.
#[derive(Clone)]
pub struct EngineHost {
    engine: EngineHandle,
}

impl EngineHost {
    /// Wraps `engine`; the handle is shared with the UI.
    pub fn new(engine: EngineHandle) -> Self {
        Self { engine }
    }

    fn origin(agent: &AgentId) -> Origin {
        Origin::agent(agent.0.clone())
    }
}

impl CommandHost for EngineHost {
    fn commands(&self) -> Vec<HostCommand> {
        self.engine
            .specs()
            .into_iter()
            .map(|spec| HostCommand { name: spec.name, description: spec.description, input_schema: spec.input_schema })
            .collect()
    }

    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String> {
        let result = self.engine.execute(name, input, Self::origin(agent)).map_err(|e| e.to_string())?;
        serde_json::to_value(result).map_err(|e| format!("internal error: could not encode the result ({e})"))
    }

    fn undo(&self, agent: &AgentId) -> Result<Value, String> {
        match self.engine.undo_own(Self::origin(agent)).map_err(|e| e.to_string())? {
            Some(tx) => Ok(json!({ "undone": tx })),
            None => Err("nothing to undo: you have no changes left to undo on this connection".into()),
        }
    }

    fn redo(&self, agent: &AgentId) -> Result<Value, String> {
        match self.engine.redo_own(Self::origin(agent)).map_err(|e| e.to_string())? {
            Some(tx) => Ok(json!({ "redone": tx })),
            None => Err("nothing to redo: undo one of your changes first".into()),
        }
    }

    fn snapshot(&self) -> Value {
        self.engine.read(|project| serde_json::to_value(project).unwrap_or_else(|e| json!({ "error": e.to_string() })))
    }
}
