//! Bounded, immutable board PNG snapshots with MCP image content.
use crate::{RegistryError, ToolError, ToolRegistry};
use base64::Engine as _;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use viamind_engine::{EngineHandle, EventCursor};
use viamind_render::SvgOptions;
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(default)]
    options: SvgOptions,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    expected_cursor: Option<EventCursor>,
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let slots = Arc::new(tokio::sync::Semaphore::new(2));
    registry.register_png("render_board_png", "Render an immutable PCB snapshot as an image/png MCP image block, with structured metadata/source_cursor and optional NEW path. Same supported geometry and options as render_board_view; transparent RGBA8, max 4194304 pixels/8 MiB, two concurrent renders per registry. Crop-relative coordinates preserve distant small features; extreme precision-limited views fail explicitly. No schematic, legacy previews or fabrication certification. expected_cursor checks the snapshot after waiting for a render slot. Does not change design/history/dirty state.", viamind_engine::input_schema::<Input>(), move |input| {
        let engine=engine.clone();
        let slots=slots.clone();
        async move {
            let input: Input=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            viamind_render::validate_png_options(&input.options).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let permit=slots.acquire_owned().await.map_err(|e|ToolError::Failed(e.to_string()))?;
            let (project,cursor,native)=crate::views::capture_board(&engine,input.expected_cursor)?;
            tokio::task::spawn_blocking(move || {
                // A cancelled client cannot release the slot while its blocking render continues.
                let _permit=permit;
                let rendered=viamind_render::board_png(&project,&input.options)?;
                let path=input.path.map(|p|viamind_render::write_png(&rendered.png,&p,native.as_deref())).transpose()?;
                Ok::<_,std::io::Error>(json!({"png_base64":base64::engine::general_purpose::STANDARD.encode(&rendered.png),"media_type":"image/png","metadata":rendered.metadata,"source_cursor":cursor,"path":path}))
            }).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(format!("could not render board PNG: {e}")))
        }
    })
}
