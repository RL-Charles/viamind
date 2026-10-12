//! Snapshot fabrication export shared with desktop and headless sessions.
use crate::{RegistryError, ToolError, ToolRegistry};
use serde_json::json;
use viamind_engine::{EngineHandle, EventCursor};
use viamind_fab::FabricationOptions;
type Export = viamind_fab::FabricationExportRequest<EventCursor>;
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register("export_fabrication", "Export an immutable snapshot to a NEW directory: two outer Gerber X2 copper layers, mask openings, explicit polygon profile/cutouts and XNC Excellon-subset through-via drill. All files metric, common model origin, X right/Y up, top view, bottom files not mirrored. Requires explicit pads and outline; no legacy preview inference, inner copper/pours, routing or fabrication certification. Existing destinations always reject. mask_expansion_mm defaults to 0 mm (0–10); all vias untented. include_silkscreen defaults false; true adds positive Legend Top/Bot files of stored straight strokes, no synthetic bodies, text, clipping or courtyard checks. Optional expected_cursor guards snapshot. Returns exported_cursor and manifest; preserves design/history/dirty state. Board-only snapshot; at most200000 primitives,4096 bytes/string,2 MiB aggregate text,128 MiB output. Shared two-snapshot process admission rejects busy requests.", viamind_fab::fabrication_schema(), move |input| {
        let engine=engine.clone();
        async move {
            let input: Export=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let (snapshot,cursor,native)=engine.with_engine(|e| {
                let cursor=e.event_cursor();
                if input.expected_cursor.is_some_and(|c|c!=cursor) { return Err(ToolError::Failed("document changed; call get_session_state and retry export".into())); }
                input.validate().map_err(|e|ToolError::InvalidInput(e.to_string()))?;
                let snapshot=viamind_fab::FabricationSnapshot::capture(e.project()).map_err(|e|ToolError::Failed(e.to_string()))?;
                Ok((snapshot,cursor,e.document_status().path))
            })?;
            let (path,manifest)=tokio::task::spawn_blocking(move || snapshot.write(FabricationOptions { mask_expansion_mm:input.mask_expansion_mm, include_silkscreen:input.include_silkscreen , include_paste: input.include_paste, include_npth: input.include_npth},&input.path,native.as_deref())).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(format!("could not export fabrication: {e}")))?;
            Ok(json!({"path":path,"manifest":manifest,"exported_cursor":cursor}))
        }
    })
}
