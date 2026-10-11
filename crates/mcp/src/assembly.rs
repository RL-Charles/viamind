//! Snapshot-based manufacturing file export tools shared by HTTP and headless MCP.
use serde_json::json;
use viamind_engine::{EngineHandle, EventCursor};

use crate::{RegistryError, ToolError, ToolRegistry};

type Export = viamind_fab::AssemblyExportRequest<EventCursor>;

/// Add read-only design export. Publication uses explicit overwrite permission.
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register(
        "export_assembly_csv",
        "Export generic Viamind BOM or placement CSV to a local path. Placement uses component origins in global mm, positive Y down, CCW degrees viewed from top; bottom keeps the stored transform. Not a manufacturer preset. Refuses existing files unless overwrite=true and always protects the active native file. Optional expected_cursor guards the exported snapshot; returns exported_cursor. Bounded to100000 components,4096 bytes/string,2 MiB aggregate text and32 MiB output. Shared two-snapshot process admission rejects busy requests. Does not change design/history/dirty state.",
        viamind_fab::assembly_schema(),
        move |input| {
            let engine = engine.clone();
            async move {
                let input: Export = serde_json::from_value(input).map_err(|error| ToolError::InvalidInput(error.to_string()))?;
                if input.path.as_os_str().is_empty() {
                    return Err(ToolError::InvalidInput("path must not be empty".into()));
                }
                let (snapshot, cursor, native) = engine.with_engine(|engine| {
                    let cursor = engine.event_cursor();
                    if input.expected_cursor.is_some_and(|expected| expected != cursor) {
                        return Err(ToolError::Failed("document changed; call get_session_state and retry the export".into()));
                    }
                    let snapshot = viamind_fab::AssemblySnapshot::capture(engine.project()).map_err(|e|ToolError::Failed(e.to_string()))?;
                    Ok((snapshot, cursor, engine.document_status().path))
                })?;
                let count = snapshot.component_count();
                let path = tokio::task::spawn_blocking(move || snapshot.write(input.kind, &input.path, input.overwrite, native.as_deref()))
                    .await.map_err(|error| ToolError::Failed(error.to_string()))?
                    .map_err(|error| ToolError::Failed(format!("could not export CSV: {error}")))?;
                Ok(json!({"path": path, "exported_cursor": cursor, "component_count": count, "kind": input.kind}))
            }
        },
    )
}
