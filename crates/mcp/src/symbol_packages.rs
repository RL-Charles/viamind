//! Detached multi-unit files and bounded package observations.
use crate::{RegistryError, ToolError, ToolRegistry};
use viamind_engine::{EngineHandle, LoadSymbolPackage, SaveSymbolPackage, SymbolPackageQuery, input_schema};

pub(crate) fn preflight_save(value: &serde_json::Value) -> Result<(), ToolError> {
    if value.get("path").and_then(serde_json::Value::as_str).is_some_and(|s| s.len() > 4096) {
        return Err(ToolError::InvalidInput("symbol package paths require at most4096 UTF-8 bytes before typed decoding".into()));
    }
    viamind_doc::symbol_package::predecode(value.get("template").ok_or_else(|| ToolError::InvalidInput("template is required".into()))?)
        .map_err(ToolError::InvalidInput)
}
pub fn register(registry: &mut ToolRegistry, engine: EngineHandle) -> Result<(), RegistryError> {
    let load = engine.clone();
    registry.register_observation("load_symbol_package", "Read a strict detached viamind-symbol-package version1 definition from a regular file≤1MiB;2..32 ordered rectangular units,1..1024 explicit unique package pins,4096UTF8 bytes/field and256KiB aggregate. Safe nonblocking same-fd regular-file check. No instance IDs, inferred hidden power, project/file mutation or partial placement. Use snapshot_symbol_sheet and place_symbol_package for complete fresh guarded placement.",input_schema::<LoadSymbolPackage>(),move|value|{
        let engine=load.clone();async move {
            if value.get("path").and_then(serde_json::Value::as_str).is_some_and(|s|s.len()>4096){return Err(ToolError::InvalidInput("path exceeds4096 UTF-8 bytes".into()));}
            let input=serde_json::from_value(value).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let result=tokio::task::spawn_blocking(move||engine.load_symbol_package(input)).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(e.to_string()))?;
            serde_json::to_value(result).map_err(|e|ToolError::Failed(e.to_string()))
        }
    })?;
    let save = engine.clone();
    registry.register("save_symbol_package", "Validate and save a NEW detached viamind-symbol-package version1 file. Borrowed unit/pin counts precede malformed-row typed decoding;2..32units/1024totalpins/4096UTF8 bytes per field/256KiB text/1MiB output/path≤4096bytes. Synchronized private staging outside engine lock; atomic no-clobber publication under latest active native-file lifecycle guard. Preserves source/native/existing files, full session/history/selection and editing drafts; never applies a definition implicitly.",input_schema::<SaveSymbolPackage>(),move|value|{
        let engine=save.clone();async move {
            preflight_save(&value)?;
            let input=serde_json::from_value(value).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let result=tokio::task::spawn_blocking(move||engine.save_symbol_package(input)).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(e.to_string()))?;
            serde_json::to_value(result).map_err(|e|ToolError::Failed(e.to_string()))
        }
    })?;
    registry.register_observation("query_symbol_packages", "Read explicit native package membership without mutation. Optional sheet_id,offset,limit1..100; deterministic(sheetUUID,packageUUID) order, captured project/full cursor, total/next_offset, shared reference/value/footprint, ordered unit/symbol/pinUUID+number/electrical/exact endpoint mapping. Bounded schematic-only capture≤10000 objects/membership/pins and2MiB text before cloning; response≤10000pins. Group identity/functional or power role does not imply electrical connections, annotation or PCB equivalence.",input_schema::<SymbolPackageQuery>(),move|value|{
        let engine=engine.clone();async move {
            let input=serde_json::from_value(value).map_err(|e|ToolError::InvalidInput(e.to_string()))?;
            let result=tokio::task::spawn_blocking(move||engine.query_symbol_packages(&input)).await.map_err(|e|ToolError::Failed(e.to_string()))?.map_err(|e|ToolError::Failed(e.to_string()))?;
            serde_json::to_value(result).map_err(|e|ToolError::Failed(e.to_string()))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn borrowed_unit_pin_and_path_counts_precede_malformed_secondary_rows() {
        for (units, needle) in [(vec![Value::Null; 33], "2..32"), (vec![json!({"pins":vec![Value::Null;1025]}), json!({"pins":[null]})], "1024")] {
            assert!(preflight_save(&json!({"path":"ok","template":{"units":units}})).unwrap_err().to_string().contains(needle));
        }
        assert!(preflight_save(&json!({"path":"x".repeat(4097),"template":null})).unwrap_err().to_string().contains("4096"));
    }
}
