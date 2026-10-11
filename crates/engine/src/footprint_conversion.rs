//! Detached file conversion; no design mutations, source overwrite or inferred nets.
use crate::{EngineError, EngineHandle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use viamind_doc::{footprint::Footprint, pcb_sexpr::FidelityReport};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewFootprintConversion {
    pub path: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConvertFootprintFile {
    pub path: PathBuf,
    pub expected_source_sha256: String,
    pub destination: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FootprintConversionPreview {
    pub path: PathBuf,
    pub source_sha256: String,
    pub can_convert: bool,
    pub footprint: Option<Footprint>,
    pub report: FidelityReport,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConvertedFootprintFile {
    pub path: PathBuf,
    pub source_sha256: String,
    pub footprint: Footprint,
    pub report: FidelityReport,
}
fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::InvalidInput { command: "convert_footprint_file".into(), message: message.into() }
}
fn remap(error: EngineError) -> EngineError {
    match error {
        EngineError::InvalidInput { message, .. } => invalid(message),
        error => invalid(error.to_string()),
    }
}
impl EngineHandle {
    /// Read one bounded source without changing design/session/status or any files.
    pub fn preview_footprint_conversion(&self, input: PreviewFootprintConversion) -> Result<FootprintConversionPreview, EngineError> {
        crate::command_hooks::check_read()?;
        let (path, bytes) = crate::pcb_import::read_source_bounded(&input.path, viamind_doc::footprint_sexpr::MAX_SOURCE_BYTES).map_err(remap)?;
        let source = std::str::from_utf8(&bytes).map_err(|_| invalid("footprint source must be UTF-8"))?;
        let parsed = viamind_doc::footprint_sexpr::parse(source).map_err(|e| invalid(e.to_string()))?;
        Ok(FootprintConversionPreview {
            path,
            source_sha256: crate::pcb_import::digest(&bytes),
            can_convert: parsed.report.can_import(),
            footprint: parsed.footprint,
            report: parsed.report,
        })
    }
    /// Reparse exact preview bytes, then publish one new native library under the file guard.
    pub fn convert_footprint_file(&self, input: ConvertFootprintFile) -> Result<ConvertedFootprintFile, EngineError> {
        crate::command_hooks::check_write()?;
        self.convert_footprint_file_with_before_publish(input, || {})
    }
    fn convert_footprint_file_with_before_publish(
        &self,
        input: ConvertFootprintFile,
        before_publish: impl FnOnce(),
    ) -> Result<ConvertedFootprintFile, EngineError> {
        if input.expected_source_sha256.len() != 64 || !input.expected_source_sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("expected_source_sha256 must be the exact lowercase preview digest"));
        }
        // Both paths are bounded before any filesystem read/canonicalization/staging.
        let source = crate::pcb_import::source_absolute_path(&input.path).map_err(remap)?;
        let destination = crate::pcb_import::source_absolute_path(&input.destination).map_err(remap)?;
        let same_canonical = match (std::fs::canonicalize(&source), std::fs::canonicalize(&destination)) {
            (Ok(source), Ok(destination)) => source == destination,
            _ => false,
        };
        if source == destination || same_canonical {
            return Err(invalid("source and destination must be separate files; source is never overwritten"));
        }
        let preview = self.preview_footprint_conversion(PreviewFootprintConversion { path: source })?;
        if preview.source_sha256 != input.expected_source_sha256 {
            return Err(invalid("source bytes changed; preview again before conversion"));
        }
        let footprint = preview
            .footprint
            .filter(|_| preview.can_convert)
            .ok_or_else(|| invalid("unsupported source geometry blocks conversion; inspect fidelity report"))?;
        let staged = viamind_doc::footprint::stage_new(&footprint, &destination).map_err(invalid)?;
        before_publish();
        let path = self.with_native_file_guard(|native| staged.publish(native)).map_err(invalid)?;
        Ok(ConvertedFootprintFile { path, source_sha256: preview.source_sha256, footprint, report: preview.report })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_native_path_is_checked_after_staging_and_peer_edits_remain_available() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("original.kicad_mod");
        std::fs::write(&source, include_str!("../../../fixtures/import/original-rect-smd.kicad_mod")).unwrap();
        let destination = directory.path().join("peer-native.viamind.json");
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let preview = engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).unwrap();
        let error = engine
            .convert_footprint_file_with_before_publish(
                ConvertFootprintFile { path: source.clone(), expected_source_sha256: preview.source_sha256, destination: destination.clone() },
                || {
                    engine
                        .execute(
                            "add_part",
                            serde_json::json!({"designator":"PEER1","value":"keep","footprint":"original","x_mm":0,"y_mm":0}),
                            crate::Origin::User,
                        )
                        .unwrap();
                    engine.save_project(crate::SaveProject { path: Some(destination.clone()), overwrite: false }).unwrap();
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("native project"));
        assert!(engine.read(|p| p.board.part_by_designator("PEER1").is_some()));
        assert_eq!(viamind_doc::file::load(&destination).unwrap(), engine.session_state().project);
        assert!(!engine.document_status().dirty);
        assert!(engine.can_undo());
        assert_eq!(std::fs::read_to_string(source).unwrap(), include_str!("../../../fixtures/import/original-rect-smd.kicad_mod"));
    }
}
