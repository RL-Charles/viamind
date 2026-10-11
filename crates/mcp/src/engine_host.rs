//! The design engine as a [`CommandHost`], so every engine command is an MCP tool.

use serde_json::{Value, json};
use viamind_engine::{EngineHandle, Origin, PartQuery, SetSelection, input_schema};

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
        let mut commands: Vec<_> = self
            .engine
            .specs()
            .into_iter()
            .map(|spec| HostCommand { name: spec.name, description: spec.description, input_schema: spec.input_schema })
            .collect();
        commands.push(HostCommand {
            name: "query_parts".into(),
            description: "Read a bounded page of board parts using combined filters (IDs, exact fields, text, side, origin bounds in mm). Read-only. UUID order; limit defaults to 100 (max 1000). Repeat filters with next_after as after for the next page; restart if the design changes. Returns parts, total matches and next_after; positions are integer nanometres.".into(),
            input_schema: input_schema::<PartQuery>(),
        });
        commands.push(HostCommand {
            name: "set_selection".into(),
            description: "Select board part IDs in the shared desktop session. Modes: replace (default), add, remove, toggle. Empty replace clears selection. Transient: preserves design undo and redo history.".into(),
            input_schema: input_schema::<SetSelection>(),
        });
        commands.push(HostCommand {
            name: "get_selection".into(),
            description: "Read the shared selection: sorted part IDs, revision and last origin.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        });
        commands
    }

    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String> {
        if name == "query_parts" {
            let query: PartQuery = serde_json::from_value(input).map_err(|e| format!("invalid query_parts input: {e}"))?;
            let result = self.engine.query_parts(&query).map_err(|e| e.to_string())?;
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        if name == "get_selection" {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err("get_selection takes an empty object".into());
            }
            return serde_json::to_value(self.engine.selection()).map_err(|e| e.to_string());
        }
        if name == "set_selection" {
            let input: SetSelection = serde_json::from_value(input).map_err(|e| format!("invalid set_selection input: {e}"))?;
            let selection = self.engine.set_selection(input, Self::origin(agent)).map_err(|e| e.to_string())?;
            return serde_json::to_value(selection).map_err(|e| e.to_string());
        }
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
