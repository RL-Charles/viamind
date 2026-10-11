//! Read current trusted policy without exposing any policy setter.
use crate::{RegistryError, ToolError, ToolRegistry};
use serde_json::json;
use viamind_engine::agent_policy::AgentMode;
pub(crate) fn register(registry: &mut ToolRegistry) -> Result<(), RegistryError> {
    let policy = registry.agent_policy();
    registry.register_with_context("get_agent_mode", "Observe current trusted agent mode, policy generation, fixed startup/endpoint restriction, active writer count and supported bounded PCB proposal commands. No policy setter exists for agents. Human desktop editing remains live; mode does not authenticate clients or isolate OS/filesystem access.", json!({"type":"object","properties":{},"additionalProperties":false}), move |input, ctx| {
        let policy = policy.clone();
        async move {
            if input != json!({}) { return Err(ToolError::InvalidInput("get_agent_mode takes an empty object".into())); }
            let status = policy.status();
            let mode = if ctx.endpoint_read_only { AgentMode::ReadOnly } else { status.mode };
            Ok(json!({"mode":mode,"policy_mode":status.mode,"generation":status.generation,"startup_locked":status.startup_locked,"endpoint_read_only":ctx.endpoint_read_only,"active_writers":status.active_writers,"proposal_commands":if mode == AgentMode::ReadOnly { &[] } else { viamind_engine::pcb_branches::COMMANDS }}))
        }
    })?;
    registry.admit_observation("get_agent_mode");
    Ok(())
}
