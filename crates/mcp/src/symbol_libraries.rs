//! Bounded native symbol files; all file work runs outside the async executor.
use crate::{RegistryError, ToolError, ToolRegistry};
use viamind_engine::{EngineHandle, ExportSymbolTemplate, LoadSymbolTemplate, SymbolSheetQuery, input_schema};

pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let snapshot = engine.clone();
    registry.register("snapshot_symbol_sheet", "Capture one bounded schematic sheet and its project UUID/full source cursor for place_symbol_template. Read-only; no unrelated board/sheet copy. At most10000 objects/10000 pins/4096 bytes per string/2MiB sheet text.", input_schema::<SymbolSheetQuery>(), move |input| {
        let engine = snapshot.clone();
        async move {
            let input = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let value = engine.snapshot_symbol_sheet(&input).map_err(|e| ToolError::Failed(e.to_string()))?;
            serde_json::to_value(value).map_err(|e| ToolError::Failed(e.to_string()))
        }
    })?;
    let load = engine.clone();
    registry.register("load_symbol_template", "Load a strict detached viamind-symbol version1 JSON library file (regular file at most1MiB,1024 pins,4096 bytes per field,256KiB aggregate text). Returns one rectangular single-unit symbol definition without instance identities, nets or links. Read-only; use snapshot_symbol_sheet and place_symbol_template for fresh guarded placement.", input_schema::<LoadSymbolTemplate>(), move |input| {
        let engine = load.clone();
        async move {
            let input = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let value = tokio::task::spawn_blocking(move || engine.load_symbol_template(input)).await.map_err(|e| ToolError::Failed(e.to_string()))?.map_err(|e| ToolError::Failed(e.to_string()))?;
            serde_json::to_value(value).map_err(|e| ToolError::Failed(e.to_string()))
        }
    })?;
    registry.register("export_symbol_template", "Export only one bounded native rectangular single-unit symbol definition to a NEW strict viamind-symbol version1 library file. Requires exact project/full source cursor and source sheet/symbol UUID. Excludes instance UUIDs/reference/placement/rotation/mirror/nets/PCB links. Atomic no-clobber publication protects the native file; returns exported_cursor. No design/history/selection change; no multi-unit or vector graphics support.", input_schema::<ExportSymbolTemplate>(), move |input| {
        let engine = engine.clone();
        async move {
            let input = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let value = tokio::task::spawn_blocking(move || engine.export_symbol_template(input)).await.map_err(|e| ToolError::Failed(e.to_string()))?.map_err(|e| ToolError::Failed(e.to_string()))?;
            serde_json::to_value(value).map_err(|e| ToolError::Failed(e.to_string()))
        }
    })
}
