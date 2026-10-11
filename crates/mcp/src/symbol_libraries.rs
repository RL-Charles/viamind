//! Bounded native symbol files; all file work runs outside the async executor.
use crate::{RegistryError, ToolError, ToolRegistry};
use viamind_engine::{EngineHandle, ExportSymbolTemplate, LoadSymbolTemplate, SaveSymbolTemplate, SymbolSheetQuery, input_schema};

/// The transport Value exists already; bound secondary typed rows and strings before decoding.
fn preflight_save(input: &serde_json::Value) -> Result<(), ToolError> {
    if input.get("path").and_then(serde_json::Value::as_str).is_some_and(|path| path.len() > 4096) {
        return Err(ToolError::InvalidInput("symbol definition paths require at most 4096 UTF-8 bytes before typed decoding".into()));
    }
    let Some(template) = input.get("template") else { return Ok(()) };
    let pins = template.get("pins").and_then(serde_json::Value::as_array);
    if pins.is_some_and(|pins| pins.len() > viamind_doc::symbol_library::MAX_PINS) {
        return Err(ToolError::InvalidInput("symbol definitions require at most 1024 pins before typed decoding".into()));
    }
    let mut total = 0_usize;
    let fields = ["name", "value", "footprint"].into_iter().filter_map(|key| template.get(key).and_then(serde_json::Value::as_str));
    let pin_fields = pins
        .into_iter()
        .flatten()
        .flat_map(|pin| ["number", "name"].into_iter().filter_map(move |key| pin.get(key).and_then(serde_json::Value::as_str)));
    for field in fields.chain(pin_fields) {
        total = total.saturating_add(field.len());
        if field.len() > 4096 || total > viamind_doc::symbol_library::MAX_TEXT_BYTES {
            return Err(ToolError::InvalidInput(
                "symbol definition text requires at most 4096 UTF-8 bytes per field and 256 KiB aggregate before typed decoding".into(),
            ));
        }
    }
    Ok(())
}

pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let save = engine.clone();
    registry.register("save_symbol_template", "Validate a detached rectangular single-unit definition and save a NEW viamind-symbol version1 file atomically. At most1024 pins/256KiB text/1MiB output. Protects native/existing files. No project, selection, history, library-instance or source-cursor mutation; explicitly load/use and guarded-place to create instances.", input_schema::<SaveSymbolTemplate>(), move |input| {
        let engine = save.clone();
        async move {
            preflight_save(&input)?;
            let input = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let value = tokio::task::spawn_blocking(move || engine.save_symbol_template(input)).await.map_err(|e| ToolError::Failed(e.to_string()))?.map_err(|e| ToolError::Failed(e.to_string()))?;
            serde_json::to_value(value).map_err(|e| ToolError::Failed(e.to_string()))
        }
    })?;
    let snapshot = engine.clone();
    registry.register_observation("snapshot_symbol_sheet", "Capture one bounded schematic sheet and its project UUID/full source cursor for place_symbol_template. Read-only; no unrelated board/sheet copy. At most10000 objects/10000 pins/4096 bytes per string/2MiB sheet text.", input_schema::<SymbolSheetQuery>(), move |input| {
        let engine = snapshot.clone();
        async move {
            let input = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let value = engine.snapshot_symbol_sheet(&input).map_err(|e| ToolError::Failed(e.to_string()))?;
            serde_json::to_value(value).map_err(|e| ToolError::Failed(e.to_string()))
        }
    })?;
    let load = engine.clone();
    registry.register_observation("load_symbol_template", "Load a strict detached viamind-symbol version1 JSON library file (regular file at most1MiB,1024 pins,4096 bytes per field,256KiB aggregate text). Returns one rectangular single-unit symbol definition without instance identities, nets or links. Read-only; use snapshot_symbol_sheet and place_symbol_template for fresh guarded placement.", input_schema::<LoadSymbolTemplate>(), move |input| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_borrowed_text_boundary_is_accepted_before_typed_semantics() {
        let mut input = json!({"path":"x".repeat(4096),"template":{"name":"","value":"","footprint":null,"pins":vec![json!({"number":"","name":"a".repeat(4096)});64]}});
        assert!(preflight_save(&input).is_ok(), "exactly 256 KiB known text and 4096 path bytes");
        input["template"]["name"] = json!("x");
        assert!(preflight_save(&input).unwrap_err().to_string().contains("256 KiB aggregate"));
        input["template"] = json!({"name":"é".repeat(2048),"pins":[]});
        assert!(preflight_save(&input).is_ok(), "limits count UTF-8 bytes");
        input["template"]["name"] = json!("é".repeat(2049));
        assert!(preflight_save(&input).is_err());
    }
}
