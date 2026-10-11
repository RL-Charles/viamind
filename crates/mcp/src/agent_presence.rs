//! Trusted transport identities own ephemeral PCB attention markers.
use serde_json::Value;
use viamind_engine::{ClearAgentPresence, EngineHandle, SetAgentPresence, input_schema};

use crate::{RegistryError, ToolError, ToolRegistry};

fn preflight(input: &Value) -> Result<(), ToolError> {
    if input.get("selection").and_then(Value::as_array).is_some_and(|ids| ids.len() > 64) {
        return Err(ToolError::Failed("presence selection requires at most 64 board IDs before typed decoding".into()));
    }
    if input.get("working_on").and_then(Value::as_str).is_some_and(|text| text.len() > 256 || text.chars().count() > 80) {
        return Err(ToolError::Failed("working_on exceeds 80 characters/256 UTF-8 bytes before typed decoding".into()));
    }
    for axis in ["x", "y"] {
        if input.get("cursor").and_then(|p| p.get(axis)).and_then(Value::as_i64).is_some_and(|nm| !(-10_000_000_000..=10_000_000_000).contains(&nm)) {
            return Err(ToolError::Failed("presence cursor requires integer Nm coordinates within +/-10000 mm".into()));
        }
    }
    Ok(())
}

pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let query = engine.clone();
    registry.register_observation("query_agent_presence", "Read bounded ephemeral PCB agent cursors, selected board IDs and working labels, paired with current project/full cursor. At most32 session-owned30second leases; observing never renews leases. No design/history/human selection/native-file change. Presence is attention, not an object lock. Takes an empty object.", serde_json::json!({"type":"object","properties":{},"additionalProperties":false}), move |input| {
        let engine = query.clone();
        async move {
            if !input.as_object().is_some_and(serde_json::Map::is_empty) { return Err(ToolError::Failed("query_agent_presence takes an empty object".into())); }
            tokio::task::spawn_blocking(move || serde_json::to_value(engine.query_agent_presence()).map_err(|e| ToolError::Failed(e.to_string())))
                .await.map_err(|e| ToolError::Failed(e.to_string()))?
        }
    })?;
    let setter = engine.clone();
    registry.register_with_context("set_agent_presence", "Publish or renew only this MCP connection's ephemeral PCB cursor/selection/working label for30seconds. Requires current expected_project and expected_stream; ordinary edits do not stale attention. Integer Nm cursor within +/-10000mm; at most64 distinct existing board part/track/via IDs; trimmed control-free working_on within80characters/256UTF8bytes. Maximum32 participants; no peer eviction. Identity comes from transport, never client names/arguments. No project/history/human selection/native-file mutation; blocked in startup read-only mode.", input_schema::<SetAgentPresence>(), move |input, ctx| {
        let engine = setter.clone();
        async move {
            let connection = ctx.connection.ok_or_else(|| ToolError::Failed("presence requires a trusted MCP connection".into()))?;
            preflight(&input)?;
            let input = serde_json::from_value(input).map_err(|e| ToolError::Failed(e.to_string()))?;
            let suffix = format!("#{connection}");
            let name = ctx.agent.0.strip_suffix(&suffix).unwrap_or(&ctx.agent.0);
            // Bound the copied presentation text independently of clientInfo length.
            let name: String = name.chars().filter(|c| !c.is_control()).take(48).collect();
            tokio::task::spawn_blocking(move || engine.set_agent_presence(connection, &name, input).and_then(|result| serde_json::to_value(result).map_err(|e| e.to_string())).map_err(ToolError::Failed))
                .await.map_err(|e| ToolError::Failed(e.to_string()))?
        }
    })?;
    registry.register_with_context("clear_agent_presence", "Clear only this trusted MCP connection's PCB attention marker. Required project/stream guards reject New/Open/reopen; missing own marker succeeds. Cannot clear a peer by forging a client name or input owner. No document/history/human selection/native-file change; blocked in startup read-only mode.", input_schema::<ClearAgentPresence>(), move |input, ctx| {
        let engine = engine.clone();
        async move {
            let connection = ctx.connection.ok_or_else(|| ToolError::Failed("presence requires a trusted MCP connection".into()))?;
            let input = serde_json::from_value(input).map_err(|e| ToolError::Failed(e.to_string()))?;
            tokio::task::spawn_blocking(move || engine.clear_agent_presence(connection, input).and_then(|result| serde_json::to_value(result).map_err(|e| e.to_string())).map_err(ToolError::Failed))
                .await.map_err(|e| ToolError::Failed(e.to_string()))?
        }
    })?;
    Ok(())
}
