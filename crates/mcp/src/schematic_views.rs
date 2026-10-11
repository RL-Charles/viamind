//! Selected-sheet inspection views share bounded capture and trusted image responses.
use crate::{RegistryError, ToolError, ToolRegistry};
use base64::Engine as _;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use viamind_doc::ObjectId;
use viamind_engine::{EngineHandle, EventCursor};
use viamind_render::SchematicSnapshot;
type Input = viamind_render::SchematicViewRequest<EventCursor>;
fn capture(
    engine: &EngineHandle,
    sheet: ObjectId,
    expected: Option<EventCursor>,
) -> Result<(SchematicSnapshot, EventCursor, Option<PathBuf>), ToolError> {
    engine.with_engine(|e| {
        let cursor = e.event_cursor();
        if expected.is_some_and(|c| c != cursor) {
            return Err(ToolError::Failed("document changed; request a fresh schematic view".into()));
        }
        let snapshot = viamind_render::schematic_view_snapshot(e.project(), sheet).map_err(|e| ToolError::Failed(e.to_string()))?;
        Ok((snapshot, cursor, e.document_status().path))
    })
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let slots = Arc::new(tokio::sync::Semaphore::new(2));
    let svg_engine = engine.clone();
    let svg_slots = Arc::clone(&slots);
    registry.register_in_memory_view("render_schematic_view","Render one explicit native schematic sheet as immutable self-contained SVG with bundled outlined text. Required sheet UUID; options select wires/symbols/pins/junctions/labels, region_nm or auto-fit, padding and bounded viewport. Pixel viewBox with absolute nm mapping in metadata. No board, other sheets, hierarchy or electrical certification. Optional exact expected_cursor and NEW path; no overwrite. Missing font glyphs fail explicitly. Does not change history/dirty state.",viamind_engine::input_schema::<Input>(),false,move|input|{
  let engine=svg_engine.clone();let slots=Arc::clone(&svg_slots);
  async move{let input:Input=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;input.options.validate().map_err(|e|ToolError::InvalidInput(e.to_string()))?;
   let permit=slots.acquire_owned().await.map_err(|e|ToolError::Failed(e.to_string()))?;
   let(snapshot,cursor,native)=capture(&engine,input.sheet,input.expected_cursor)?;
   tokio::task::spawn_blocking(move||{let _permit=permit;let rendered=viamind_render::schematic_svg(&snapshot,&input.options)?;
    let path=input.path.map(|p|viamind_render::write_svg(&rendered.svg,&p,native.as_deref())).transpose()?;
    Ok::<_,std::io::Error>(json!({"media_type":"image/svg+xml","svg":rendered.svg,"metadata":rendered.metadata,"source_cursor":cursor,"path":path}))
   }).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(format!("could not render schematic SVG: {e}")))
  }
 })?;
    registry.register_in_memory_view("render_schematic_png","Render one native schematic sheet as an actual image/png MCP image block plus structured source/view metadata. Same bounded bundled-font geometry/options as render_schematic_view; no system/external resources, 4194304 pixels/8 MiB, two shared schematic jobs per registry. Required sheet, optional exact expected_cursor and NEW file path. Transparent RGBA8, no overwrite, no board/hierarchy/electrical certification; document remains unchanged.",viamind_engine::input_schema::<Input>(),true,move|input|{
  let engine=engine.clone();let slots=Arc::clone(&slots);
  async move{let input:Input=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;input.options.validate().map_err(|e|ToolError::InvalidInput(e.to_string()))?;
   let permit=slots.acquire_owned().await.map_err(|e|ToolError::Failed(e.to_string()))?;
   let(snapshot,cursor,native)=capture(&engine,input.sheet,input.expected_cursor)?;
   tokio::task::spawn_blocking(move||{let _permit=permit;let rendered=viamind_render::schematic_png(&snapshot,&input.options)?;
    let path=input.path.map(|p|viamind_render::write_png(&rendered.png,&p,native.as_deref())).transpose()?;
    Ok::<_,std::io::Error>(json!({"png_base64":base64::engine::general_purpose::STANDARD.encode(&rendered.png),"media_type":"image/png","metadata":rendered.metadata,"source_cursor":cursor,"path":path}))
   }).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(format!("could not render schematic PNG: {e}")))
  }
 })
}
