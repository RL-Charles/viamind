//! Snapshot-based manufacturing file export tools shared by HTTP and headless MCP.
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;
use viamind_engine::{EngineHandle, EventCursor};
use viamind_fab::AssemblyCsv;

use crate::{RegistryError, ToolError, ToolRegistry};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    kind: AssemblyCsv,
    path: PathBuf,
    #[serde(default)]
    overwrite: bool,
    #[serde(default)]
    expected_cursor: Option<EventCursor>,
}

/// Add read-only design export. Publication uses explicit overwrite permission.
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register(
        "export_assembly_csv",
        "Export generic Viamind BOM or placement CSV to a local path. Placement uses component origins in global mm, positive Y down, CCW degrees viewed from top; bottom keeps the stored transform. Not a manufacturer preset. Refuses existing files unless overwrite=true and always protects the active native file. Optional expected_cursor guards the exported snapshot; returns exported_cursor. Does not change design/history/dirty state.",
        json!({
            "type": "object", "additionalProperties": false,
            "required": ["kind", "path"],
            "properties": {
                "kind": {"type": "string", "enum": ["bom", "placement"]},
                "path": {"type": "string", "minLength": 1},
                "overwrite": {"type": "boolean", "default": false},
                "expected_cursor": {"type": ["object", "null"], "additionalProperties": false,
                    "required": ["stream_id", "sequence"], "properties": {
                        "stream_id": {"type": "string", "format": "uuid"},
                        "sequence": {"type": "integer", "minimum": 0}
                    }}
            }
        }),
        move |input| {
            let engine = engine.clone();
            async move {
                let input: Export = serde_json::from_value(input).map_err(|error| ToolError::InvalidInput(error.to_string()))?;
                if input.path.as_os_str().is_empty() {
                    return Err(ToolError::InvalidInput("path must not be empty".into()));
                }
                let (snapshot, native) = engine.with_engine(|engine| (engine.session_state(), engine.document_status().path));
                if input.expected_cursor.is_some_and(|expected| expected != snapshot.cursor) {
                    return Err(ToolError::Failed("document changed; call get_session_state and retry the export".into()));
                }
                let cursor = snapshot.cursor;
                let count = snapshot.project.board.parts.len();
                let path = tokio::task::spawn_blocking(move || viamind_fab::write_csv(&snapshot.project, input.kind, &input.path, input.overwrite, native.as_deref()))
                    .await.map_err(|error| ToolError::Failed(error.to_string()))?
                    .map_err(|error| ToolError::Failed(format!("could not export CSV: {error}")))?;
                Ok(json!({"path": path, "exported_cursor": cursor, "component_count": count, "kind": input.kind}))
            }
        },
    )
}
