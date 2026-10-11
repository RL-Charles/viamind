//! Read-only, cursor-guarded board SVG snapshot tool.
use crate::{RegistryError, ToolError, ToolRegistry};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use viamind_engine::{EngineHandle, EventCursor};
use viamind_render::SvgOptions;
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(default)]
    options: SvgOptions,
    /// Optional NEW file destination; existing destinations are always preserved.
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    expected_cursor: Option<EventCursor>,
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register("render_board_view", "Render an immutable PCB snapshot as self-contained SVG (image/svg+xml). options selects outer copper/outline layers, explicit region_nm or auto-fit, padding_nm and viewport pixels. Explicit pads/tracks/vias/polygon cutouts only; legacy previews omitted and reported, no schematic/PNG/fabrication certification. Optional expected_cursor guards snapshot. Optional path publishes a NEW file with native/collision protection. Returns SVG, metadata and source_cursor without changing document/history/dirty state.", viamind_engine::input_schema::<Input>(), move |input| {
        let engine=engine.clone();
        async move {
            let input: Input=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            input.options.validate().map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let (snapshot,native)=engine.with_engine(|e|(e.session_state(),e.document_status().path));
            if input.expected_cursor.is_some_and(|c|c!=snapshot.cursor){return Err(ToolError::Failed("document changed; request a fresh board view".into()));}
            let cursor=snapshot.cursor;
            let (rendered,path)=tokio::task::spawn_blocking(move || {
                let rendered=viamind_render::board_svg(&snapshot.project,&input.options)?;
                let path=input.path.map(|p|viamind_render::write_svg(&rendered.svg,&p,native.as_deref())).transpose()?;
                Ok::<_,std::io::Error>((rendered,path))
            }).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(format!("could not render board view: {e}")))?;
            Ok(json!({"media_type":"image/svg+xml","svg":rendered.svg,"metadata":rendered.metadata,"source_cursor":cursor,"path":path}))
        }
    })
}
