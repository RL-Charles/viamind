//! Snapshot fabrication export shared with desktop and headless sessions.
use crate::{RegistryError, ToolError, ToolRegistry};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use viamind_engine::{EngineHandle, EventCursor};
use viamind_fab::FabricationOptions;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    path: PathBuf,
    #[serde(default)]
    mask_expansion_mm: f64,
    #[serde(default)]
    expected_cursor: Option<EventCursor>,
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register("export_fabrication", "Export an immutable snapshot to a NEW directory: two outer Gerber X2 copper layers, mask openings, explicit polygon profile/cutouts and XNC Excellon-subset through-via drill. All files metric, common model origin, X right/Y up, top view, bottom files not mirrored. Requires explicit pads and outline; no legacy preview inference, inner copper/pours, routing or fabrication certification. Existing destinations always reject. mask_expansion_mm defaults to 0 mm (0–10); all vias untented. Optional expected_cursor guards snapshot. Returns exported_cursor and manifest; preserves design/history/dirty state.", viamind_fab::fabrication_schema(), move |input| {
        let engine=engine.clone();
        async move {
            let input: Export=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let (snapshot,native)=engine.with_engine(|e|(e.session_state(),e.document_status().path));
            if input.expected_cursor.is_some_and(|c|c!=snapshot.cursor) { return Err(ToolError::Failed("document changed; call get_session_state and retry export".into())); }
            let cursor=snapshot.cursor;
            let (path,manifest)=tokio::task::spawn_blocking(move || viamind_fab::write_fabrication(&snapshot.project,FabricationOptions { mask_expansion_mm:input.mask_expansion_mm },&input.path,native.as_deref())).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(format!("could not export fabrication: {e}")))?;
            Ok(json!({"path":path,"manifest":manifest,"exported_cursor":cursor}))
        }
    })
}
