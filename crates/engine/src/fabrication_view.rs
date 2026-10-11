//! Immutable bounded file geometry, captured outside the document mutex.
use crate::{EngineError, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use viamind_doc::{
    ObjectId,
    fabrication_view::{self as view, FabricationGeometry, FabricationReadBudget},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectFabricationFiles {
    pub paths: Vec<PathBuf>,
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InspectedFabricationFile {
    pub path: PathBuf,
    pub source_sha256: String,
    pub geometry: FabricationGeometry,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FabricationInspection {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub files: Vec<InspectedFabricationFile>,
}
fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::InvalidInput { command: "inspect_fabrication_files".into(), message: message.into() }
}
static READING: AtomicBool = AtomicBool::new(false);
struct Permit;
impl Permit {
    fn acquire() -> Result<Self, EngineError> {
        READING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| invalid("fabrication inspection busy; retry after the current reader finishes"))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        READING.store(false, Ordering::Release);
    }
}
fn absolute(path: &Path) -> Result<PathBuf, EngineError> {
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("fabrication path must contain 1..4096 encoded bytes"));
    }
    let path = std::path::absolute(path).map_err(|e| invalid(e.to_string()))?;
    if path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("absolute fabrication path exceeds 4096 bytes"));
    }
    Ok(path)
}
fn read(path: &Path, before_open: impl FnOnce()) -> Result<Vec<u8>, EngineError> {
    before_open();
    let fd =
        rustix::fs::open(path, rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC, rustix::fs::Mode::empty())
            .map_err(|e| invalid(format!("cannot open fabrication file: {e}")))?;
    let file = File::from(fd);
    let opened = file.metadata().map_err(|e| invalid(e.to_string()))?;
    if !opened.is_file() || opened.len() > view::MAX_FILE_BYTES as u64 {
        return Err(invalid("fabrication source must be a regular file at most 4 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(view::MAX_FILE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| invalid(e.to_string()))?;
    let after = std::fs::metadata(path).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() > view::MAX_FILE_BYTES
        || bytes.len() as u64 != opened.len()
        || !after.is_file()
        || after.len() != opened.len()
        || after.modified().ok() != opened.modified().ok()
    {
        return Err(invalid("fabrication source changed while reading; inspect again"));
    }
    Ok(bytes)
}
impl EngineHandle {
    pub fn inspect_fabrication_files(&self, input: InspectFabricationFiles) -> Result<FabricationInspection, EngineError> {
        self.inspect_fabrication_files_with(input, || {})
    }
    fn inspect_fabrication_files_with(
        &self,
        input: InspectFabricationFiles,
        after_capture: impl FnOnce(),
    ) -> Result<FabricationInspection, EngineError> {
        if !(1..=6).contains(&input.paths.len()) {
            return Err(invalid("choose between one and six fabrication files"));
        }
        let mut unique = std::collections::BTreeSet::new();
        let paths = input.paths.iter().map(|p| absolute(p)).collect::<Result<Vec<_>, _>>()?;
        for path in &paths {
            if !unique.insert(path) {
                return Err(invalid("duplicate fabrication file path"));
            }
        }
        let _permit = Permit::acquire()?;
        self.with_engine(|e| {
            if e.project().id != input.expected_project || e.event_cursor() != input.expected_cursor {
                return Err(invalid("document changed; refresh project and full cursor before inspection"));
            }
            Ok(())
        })?;
        after_capture();
        let mut budget = FabricationReadBudget::default();
        let mut total = 0usize;
        let mut files = Vec::new();
        for path in paths {
            let bytes = read(&path, || {})?;
            total = total.saturating_add(bytes.len());
            if total > view::MAX_TOTAL_BYTES {
                return Err(invalid("fabrication sources exceed 16 MiB aggregate"));
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| invalid("fabrication source must be ASCII text"))?;
            let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let geometry = if extension.eq_ignore_ascii_case("gbr") {
                view::parse_gerber(text, &mut budget)
            } else if extension.eq_ignore_ascii_case("xnc") {
                view::parse_xnc(text, &mut budget)
            } else {
                return Err(invalid("choose explicit .gbr or .xnc files"));
            }
            .map_err(invalid)?;
            if geometry.same_coordinates.is_some_and(|id| id != input.expected_project) {
                return Err(invalid("fabrication SameCoordinates UUID differs from the requested project"));
            }
            let source_sha256 = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            files.push(InspectedFabricationFile { path, source_sha256, geometry });
        }
        Ok(FabricationInspection { expected_project: input.expected_project, source_cursor: input.expected_cursor, files })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_admission_is_nonwaiting_and_releases_on_error_and_drop() {
        let first = Permit::acquire().unwrap();
        assert!(Permit::acquire().is_err());
        drop(first);
        drop(Permit::acquire().unwrap());
        let engine = EngineHandle::new(viamind_doc::Project::new("inspection"));
        let input = InspectFabricationFiles {
            paths: vec![PathBuf::from("/missing/inspection.gbr")],
            expected_project: engine.read(|p| p.id),
            expected_cursor: engine.event_cursor(),
        };
        assert!(engine.inspect_fabrication_files(input).is_err());
        drop(Permit::acquire().unwrap());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn raced_fifo_read_never_waits_for_a_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.gbr");
        std::fs::write(&path, "original").unwrap();
        let error = read(&path, || {
            std::fs::remove_file(&path).unwrap();
            rustix::fs::mkfifoat(rustix::fs::CWD, &path, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR).unwrap();
        })
        .unwrap_err();
        assert!(error.to_string().contains("regular file"));
    }
}
