//! Bounded regular-file PCB conversion with immutable preview and guarded lifecycle replacement.
use crate::{DocumentStatus, EngineError, EngineHandle, EventCursor, Origin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use viamind_doc::{
    ObjectId,
    pcb_sexpr::{FidelityReport, MAX_SOURCE_BYTES, ParsedPcb},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewPcbImport {
    pub path: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPcb {
    pub path: PathBuf,
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub expected_source_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PcbImportPreview {
    pub path: PathBuf,
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub source_sha256: String,
    pub can_import: bool,
    pub report: FidelityReport,
}
#[derive(Clone, Debug, Serialize)]
pub struct PcbImportResult {
    pub project_id: ObjectId,
    pub cursor: EventCursor,
    pub document: DocumentStatus,
    pub report: FidelityReport,
}
fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::InvalidInput { command: "import_pcb".into(), message: message.into() }
}
fn read_source(path: &Path) -> Result<(PathBuf, Vec<u8>), EngineError> {
    read_source_with_before_open(path, || {})
}
fn read_source_with_before_open(path: &Path, before_open: impl FnOnce()) -> Result<(PathBuf, Vec<u8>), EngineError> {
    if path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("source path exceeds 4096 encoded bytes"));
    }
    if path.as_os_str().is_empty() {
        return Err(invalid("source path must not be empty"));
    }
    let path = std::path::absolute(path).map_err(|e| invalid(e.to_string()))?;
    if path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("absolute source path exceeds 4096 encoded bytes"));
    }
    let before = std::fs::metadata(&path).map_err(|e| invalid(format!("cannot inspect source: {e}")))?;
    if !before.is_file() || before.len() > MAX_SOURCE_BYTES as u64 {
        return Err(invalid("source must be a regular file at most 8 MiB; FIFOs/devices/directories are unsupported"));
    }
    before_open();
    // NONBLOCK prevents a regular-file→FIFO replacement from blocking this worker.
    let fd =
        rustix::fs::open(&path, rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC, rustix::fs::Mode::empty())
            .map_err(|e| invalid(format!("cannot open source: {e}")))?;
    let file = File::from(fd);
    let opened = file.metadata().map_err(|e| invalid(e.to_string()))?;
    if !opened.is_file() || opened.len() > MAX_SOURCE_BYTES as u64 {
        return Err(invalid("opened source must be a regular file at most 8 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| invalid(e.to_string()))?;
    let after = std::fs::metadata(&path).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() > MAX_SOURCE_BYTES
        || bytes.len() as u64 != opened.len()
        || !after.is_file()
        || after.len() != opened.len()
        || after.modified().ok() != opened.modified().ok()
    {
        return Err(invalid("source changed while reading or exceeded 8 MiB; preview again"));
    }
    Ok((path, bytes))
}
fn parse(bytes: &[u8]) -> Result<ParsedPcb, EngineError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("source must be UTF-8 text"))?;
    viamind_doc::pcb_sexpr::parse(text).map_err(|e| invalid(e.to_string()))
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
impl EngineHandle {
    /// Conversion outside the engine lock; preview never changes document/history/path/selection.
    pub fn preview_pcb_import(&self, input: PreviewPcbImport) -> Result<PcbImportPreview, EngineError> {
        let (expected_project, expected_cursor) = self.with_engine(|e| (e.project().id, e.event_cursor()));
        let (path, bytes) = read_source(&input.path)?;
        let source_sha256 = digest(&bytes);
        let parsed = parse(&bytes)?;
        Ok(PcbImportPreview { path, expected_project, expected_cursor, source_sha256, can_import: parsed.report.can_import(), report: parsed.report })
    }
    /// Replace only a clean exact session from the same exact preview bytes; never write source.
    pub fn import_pcb(&self, input: ImportPcb, origin: Origin) -> Result<PcbImportResult, EngineError> {
        if input.expected_source_sha256.len() != 64 || !input.expected_source_sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("expected_source_sha256 must be the exact lowercase preview digest"));
        }
        let mut report = None;
        let snapshot = self.replace_imported_project(input.expected_project, input.expected_cursor, origin, || {
            let (_, bytes) = read_source(&input.path)?;
            if digest(&bytes) != input.expected_source_sha256 {
                return Err(invalid("source bytes changed; preview again"));
            }
            let parsed = parse(&bytes)?;
            if !parsed.report.can_import() {
                return Err(invalid("unsupported or invalid geometry blocks import; inspect preview fidelity report"));
            }
            report = Some(parsed.report);
            Ok(parsed.project)
        })?;
        Ok(PcbImportResult {
            project_id: snapshot.project.id,
            cursor: snapshot.cursor,
            document: DocumentStatus { path: None, dirty: true },
            report: report.expect("successful guarded import parsed report"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn raced_fifo_open_returns_without_waiting_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source");
        std::fs::write(&path, "original").unwrap();
        let result = read_source_with_before_open(&path, || {
            std::fs::remove_file(&path).unwrap();
            assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
        });
        assert!(result.unwrap_err().to_string().contains("regular file"));
    }
    #[test]
    fn peer_change_during_import_load_rejects_without_replacement() {
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let before = engine.session_state();
        let result = engine.replace_imported_project(before.project.id, before.cursor, Origin::User, || {
            engine
                .execute("add_part", serde_json::json!({"designator":"PEER1","value":"keep","footprint":"original","x_mm":0,"y_mm":0}), Origin::User)
                .unwrap();
            Ok(viamind_doc::Project::new("must not replace"))
        });
        assert!(result.is_err());
        assert_eq!(engine.session_state().project.id, before.project.id);
        assert!(engine.session_state().project.board.part_by_designator("PEER1").is_some());
        assert!(engine.document_status().dirty);
        assert!(engine.can_undo());
    }
}
