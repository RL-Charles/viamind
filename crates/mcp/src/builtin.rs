//! Built-in tools that exist independently of the engine.

use serde_json::{Value, json};

use crate::{
    SERVER_NAME,
    registry::{RegistryError, ToolRegistry},
};

/// Register `viamind_version` and `echo`.
pub fn register_builtins(registry: &mut ToolRegistry) -> Result<(), RegistryError> {
    registry.register_observation(
        "viamind_version",
        "Return the name and version of the running Viamind MCP server.",
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        |_input| async {
            Ok(json!({
                "name": SERVER_NAME,
                "version": env!("CARGO_PKG_VERSION"),
            }))
        },
    )?;
    registry.register_observation(
        "echo",
        "Return the arguments unchanged. Useful to check that the connection works.",
        json!({
            "type": "object",
            "description": "Any JSON object; it is returned as-is.",
            "additionalProperties": true
        }),
        |input: Value| async move { Ok(input) },
    )?;
    Ok(())
}
