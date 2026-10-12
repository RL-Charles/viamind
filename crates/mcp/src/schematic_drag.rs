//! Retain preview admission through actual bounded worker retirement.
use crate::{RegistryError, ToolError, ToolRegistry};
use viamind_engine::{DragSchematicSymbol, EngineHandle, input_schema};
pub(crate) fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    registry.register_observation(
        "preview_schematic_symbol_drag",
        "Preview one explicitly guarded integer-Nm schematic symbol translation and cardinal endpoint-wire adjustment without changing model/history/selection/native. Required project UUID/full source cursor/sheet/symbol UUID. Exact complete original wire/pin/label/junction partition and label scopes must remain equal. Whole graph: 10000 objects/2 MiB text, 100000 combined identities, 256 selected pins/incident wires, shared1M visits, 2 MiB output and two retained planners; unsupported attachments/topology/budget refuse. New segments use local ordinals, not permanent IDs. Apply with drag_schematic_symbol, which recalculates against its guarded source. Available in all agent modes. Not ERC/DRC certification.",
        input_schema::<DragSchematicSymbol>(),
        move|input| {let engine=engine.clone();async move {
            let request=serde_json::from_value(input).map_err(|e|ToolError::InvalidInput(format!("invalid connected drag request: {e}")))?;
            let work=engine.prepare_schematic_symbol_drag(request).map_err(|e|ToolError::Failed(e.to_string()))?;
            tokio::task::spawn_blocking(move||work.run_value()).await.map_err(|e|ToolError::Failed(format!("connected drag worker stopped: {e}")))?.map_err(|e|ToolError::Failed(e.to_string()))
        }}
    )
}
