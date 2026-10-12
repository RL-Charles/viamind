//! Shared bounded preview and guarded Apply of certified circular DXF outline conversion.
//!
//! Source reads and conversion run outside the design lock. Preview is an observation;
//! Apply rereads and reparses the source, compares its SHA-256 and recomputes the same
//! bounded plan, then commits one ordinary BoardOutline delta under the full project,
//! cursor and exact-outline guards. The straight-only `import_board_outline_dxf`
//! command is unchanged.
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;
use viamind_doc::{
    BoardOutline, ObjectId, Project,
    dxf::DxfUnits,
    dxf_curves::{self as curves, DxfConversionReport},
};

use crate::{Command, CommandError, Delta, EngineError, EngineHandle, EventCursor, ExecResult, ExpectedOutline, Origin, pad_apertures::StrictValue};

pub const PREVIEW_TOOL: &str = "preview_board_outline_dxf";
pub const APPLY_TOOL: &str = "apply_board_outline_dxf";

/// Read-only certified conversion of exactly one bounded DXF source.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewBoardOutlineDxf {
    /// Inline ASCII/UTF-8 DXF text (at most 8 MiB). Supply exactly one of `text` or `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 8_388_608))]
    pub text: Option<String>,
    /// Local regular DXF file path (at most 4096 encoded bytes), read through one nonblocking descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// `header` (default) requires $INSUNITS; `mm` or `inches` explicitly override it.
    #[serde(default)]
    pub units: DxfUnits,
    /// Optional exact layer name (1..512 bytes); POLYLINE vertices follow their parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Required explicit integer chord tolerance in Nm, 10..=1000000.
    #[schemars(range(min = 10, max = 1_000_000))]
    pub tolerance_nm: u64,
}

/// Guarded Apply of the same source/options under exact preview guards.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyBoardOutlineDxf {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 8_388_608))]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub units: DxfUnits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[schemars(range(min = 10, max = 1_000_000))]
    pub tolerance_nm: u64,
    /// Lowercase SHA-256 of the previewed source bytes.
    pub expected_source_sha256: String,
    /// Full session cursor (stream and sequence) from the preview.
    pub expected_cursor: EventCursor,
    /// Exact project identity and current outline from the preview.
    pub expected: ExpectedOutline,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPreview {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    units: DxfUnits,
    #[serde(default)]
    layer: Option<String>,
    tolerance_nm: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApply {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    units: DxfUnits,
    #[serde(default)]
    layer: Option<String>,
    tolerance_nm: u64,
    expected_source_sha256: String,
    expected_cursor: EventCursor,
    expected: ExpectedOutline,
}

/// Borrowed admission of raw input sizes before typed secondary allocation.
///
/// A transport that already produced a `Value` has discarded duplicate keys; raw JSON
/// decoded through these types' `Deserialize` rejects duplicates at every object.
pub fn preflight_dxf_outline_input(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("DXF outline input must be a JSON object")?;
    let text = |key: &str, max: usize| {
        if object.get(key).and_then(Value::as_str).is_some_and(|s| s.len() > max) {
            Err(format!("{key} exceeds {max} bytes before typed decoding"))
        } else {
            Ok(())
        }
    };
    text("text", curves::MAX_SOURCE_BYTES)?;
    text("path", curves::MAX_PATH_BYTES)?;
    text("layer", curves::MAX_LAYER_BYTES)?;
    text("expected_source_sha256", 64)?;
    if let Some(outline) = object.get("expected").and_then(|e| e.get("outline")).filter(|o| !o.is_null()) {
        let cutouts = outline.get("cutouts").and_then(Value::as_array);
        if cutouts.is_some_and(|c| c.len() > viamind_doc::outline::MAX_OUTLINE_CUTOUTS) {
            return Err("expected outline has more than 64 cutouts before typed decoding".into());
        }
        let points = outline.get("outer").and_then(Value::as_array).map_or(0, Vec::len)
            + cutouts.map_or(0, |c| c.iter().map(|r| r.as_array().map_or(0, Vec::len)).sum());
        if points > curves::MAX_VERTICES {
            return Err("expected outline has more than 2048 vertices before typed decoding".into());
        }
    }
    Ok(())
}
fn strict<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(d: D) -> Result<T, D::Error> {
    let value = StrictValue::deserialize(d)?.0;
    preflight_dxf_outline_input(&value).map_err(de::Error::custom)?;
    serde_json::from_value(value).map_err(de::Error::custom)
}
impl<'de> Deserialize<'de> for PreviewBoardOutlineDxf {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r: RawPreview = strict(d)?;
        Ok(Self { text: r.text, path: r.path, units: r.units, layer: r.layer, tolerance_nm: r.tolerance_nm })
    }
}
impl<'de> Deserialize<'de> for ApplyBoardOutlineDxf {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r: RawApply = strict(d)?;
        Ok(Self {
            text: r.text,
            path: r.path,
            units: r.units,
            layer: r.layer,
            tolerance_nm: r.tolerance_nm,
            expected_source_sha256: r.expected_source_sha256,
            expected_cursor: r.expected_cursor,
            expected: r.expected,
        })
    }
}

/// Which kind of source a preview read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DxfSourceKind {
    Text,
    Path,
}

/// Deterministic conversion plus runtime guards for a later Apply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DxfOutlinePreview {
    pub source: DxfSourceKind,
    /// Absolute path that was read, for path sources.
    pub path: Option<PathBuf>,
    pub source_bytes: u64,
    pub source_sha256: String,
    /// Project and full session cursor captured before the read (runtime metadata).
    pub expected_cursor: EventCursor,
    pub expected: ExpectedOutline,
    /// True when applying would replace the current outline.
    pub changes_outline: bool,
    pub outline: BoardOutline,
    pub report: DxfConversionReport,
}

/// Outcome of a guarded Apply: one BoardOutline delta, or a no-op for equal geometry.
#[derive(Clone, Debug, Serialize)]
pub struct DxfOutlineApplied {
    pub source_sha256: String,
    pub changed: bool,
    pub cursor: EventCursor,
    pub achieved_bound_nm: u64,
    pub vertices: u64,
    pub cutouts: u64,
    pub result: ExecResult,
}

fn invalid(command: &str, message: impl Into<String>) -> EngineError {
    let mut message = message.into();
    if message.len() > curves::MAX_DIAGNOSTIC_BYTES {
        let mut end = curves::MAX_DIAGNOSTIC_BYTES - 3;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str("...");
    }
    EngineError::InvalidInput { command: command.into(), message }
}

/// Read one local DXF through a single nonblocking close-on-exec descriptor.
///
/// Metadata of that same descriptor must describe a regular file; FIFOs, devices and
/// directories (including ones raced in after the path was chosen) refuse without
/// waiting. At most limit+1 bytes are read and must be UTF-8. The digest binds these
/// bytes, not the path's later identity.
pub(crate) fn read_dxf_path(command: &str, path: &Path, before_open: impl FnOnce()) -> Result<(PathBuf, String), EngineError> {
    let encoded = path.as_os_str().as_encoded_bytes().len();
    if encoded == 0 || encoded > curves::MAX_PATH_BYTES {
        return Err(invalid(command, "path must be 1..4096 encoded bytes"));
    }
    let path = std::path::absolute(path).map_err(|e| invalid(command, format!("cannot resolve path: {e}")))?;
    if path.as_os_str().as_encoded_bytes().len() > curves::MAX_PATH_BYTES {
        return Err(invalid(command, "absolute path exceeds 4096 encoded bytes"));
    }
    before_open();
    let fd =
        rustix::fs::open(&path, rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC, rustix::fs::Mode::empty())
            .map_err(|e| invalid(command, format!("cannot open DXF source: {e}")))?;
    let file = File::from(fd);
    let metadata = file.metadata().map_err(|e| invalid(command, format!("cannot inspect opened DXF source: {e}")))?;
    if !metadata.is_file() {
        return Err(invalid(command, "opened DXF source must be a regular file; FIFOs, devices and directories are unsupported"));
    }
    if metadata.len() > curves::MAX_SOURCE_BYTES as u64 {
        return Err(invalid(command, "DXF source exceeds the 8 MiB limit"));
    }
    let mut bytes = Vec::new();
    file.take(curves::MAX_SOURCE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| invalid(command, format!("cannot read DXF source: {e}")))?;
    if bytes.len() > curves::MAX_SOURCE_BYTES {
        return Err(invalid(command, "DXF source exceeds the 8 MiB limit"));
    }
    let text = String::from_utf8(bytes).map_err(|_| invalid(command, "DXF source must be ASCII/UTF-8 text"))?;
    Ok((path, text))
}

struct Planned {
    source: DxfSourceKind,
    path: Option<PathBuf>,
    bytes: u64,
    sha256: String,
    conversion: curves::DxfConversion,
}

/// Source admission, read, digest and certified conversion; never touches the design.
fn plan(
    command: &str,
    text: Option<&str>,
    path: Option<&Path>,
    units: DxfUnits,
    layer: Option<&str>,
    tolerance: u64,
) -> Result<Planned, EngineError> {
    if !(curves::MIN_TOLERANCE_NM..=curves::MAX_TOLERANCE_NM).contains(&tolerance) {
        return Err(invalid(command, "tolerance_nm must be an explicit integer in [10, 1000000]"));
    }
    if layer.is_some_and(|l| l.is_empty() || l.len() > curves::MAX_LAYER_BYTES) {
        return Err(invalid(command, "layer must be 1..512 bytes, or omitted for all layers"));
    }
    let (source, path, owned) = match (text, path) {
        (Some(text), None) => {
            if text.len() > curves::MAX_SOURCE_BYTES {
                return Err(invalid(command, "text exceeds the 8 MiB limit"));
            }
            (DxfSourceKind::Text, None, None)
        }
        (None, Some(path)) => {
            let (absolute, text) = read_dxf_path(command, path, || {})?;
            (DxfSourceKind::Path, Some(absolute), Some(text))
        }
        _ => return Err(invalid(command, "supply exactly one source: text or path")),
    };
    let text = owned.as_deref().or(text).unwrap_or_default();
    let sha256 = crate::pcb_import::digest(text.as_bytes());
    let conversion = curves::convert_outline_dxf(text, units, layer, tolerance).map_err(|e| invalid(command, e.to_string()))?;
    Ok(Planned { source, path, bytes: text.len() as u64, sha256, conversion })
}

/// Bounded counting serializer: refuse oversized responses before publication.
fn within_response_budget(command: &str, value: &impl Serialize) -> Result<(), EngineError> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(b.len());
            if self.0 > curves::MAX_REPORT_BYTES {
                return Err(std::io::Error::other("response exceeds 2 MiB"));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter(0), value)
        .map_err(|_| invalid(command, "serialized preview/report plus outline exceeds 2 MiB; filter a layer or simplify"))
}

/// Internal guarded commit of an already recomputed plan; not a registered agent command.
#[derive(Serialize, Deserialize, JsonSchema)]
struct CommitDxfOutline {
    expected_project: ObjectId,
    expected_cursor: EventCursor,
    expected_outline: Option<BoardOutline>,
    outline: BoardOutline,
}
impl Command for CommitDxfOutline {
    const NAME: &'static str = APPLY_TOOL;
    const DESCRIPTION: &'static str = "Internal guarded commit of a recomputed certified DXF outline plan.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project || project.board.outline != self.expected_outline {
            return Err(CommandError::conflict("board outline or document changed; preview the DXF again"));
        }
        if project.board.outline.as_ref() == Some(&self.outline) {
            return Ok(Vec::new());
        }
        let before = project.board.outline.replace(self.outline.clone());
        Ok(vec![Delta::BoardOutline { id: project.id, before, after: Some(self.outline) }])
    }
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl EngineHandle {
    /// Observation: capture guards, then read/convert outside the design lock.
    pub fn preview_board_outline_dxf(&self, input: &PreviewBoardOutlineDxf) -> Result<DxfOutlinePreview, EngineError> {
        crate::command_hooks::check_read()?;
        let (cursor, expected) =
            self.with_engine(|e| (e.event_cursor(), ExpectedOutline { project_id: e.project().id, outline: e.project().board.outline.clone() }));
        let planned = plan(PREVIEW_TOOL, input.text.as_deref(), input.path.as_deref(), input.units, input.layer.as_deref(), input.tolerance_nm)?;
        let preview = DxfOutlinePreview {
            source: planned.source,
            path: planned.path,
            source_bytes: planned.bytes,
            source_sha256: planned.sha256,
            expected_cursor: cursor,
            changes_outline: expected.outline.as_ref() != Some(&planned.conversion.outline),
            expected,
            outline: planned.conversion.outline,
            report: planned.conversion.report,
        };
        within_response_budget(PREVIEW_TOOL, &preview)?;
        Ok(preview)
    }

    /// Writer: reread/reparse and recompute outside the lock, then commit one guarded delta.
    pub fn apply_board_outline_dxf(&self, input: &ApplyBoardOutlineDxf, origin: Origin) -> Result<DxfOutlineApplied, EngineError> {
        crate::command_hooks::check_write()?;
        if !valid_digest(&input.expected_source_sha256) {
            return Err(invalid(APPLY_TOOL, "expected_source_sha256 must be the exact lowercase preview digest"));
        }
        let current = self.with_engine(|e| (e.event_cursor(), e.project().id, e.project().board.outline == input.expected.outline));
        if current != (input.expected_cursor, input.expected.project_id, true) {
            return Err(invalid(APPLY_TOOL, "document, cursor or outline changed since preview; preview again"));
        }
        let planned = plan(APPLY_TOOL, input.text.as_deref(), input.path.as_deref(), input.units, input.layer.as_deref(), input.tolerance_nm)?;
        if planned.sha256 != input.expected_source_sha256 {
            return Err(invalid(APPLY_TOOL, "source bytes changed since preview; preview again"));
        }
        let report = planned.conversion.report;
        let result = self.execute_typed(
            CommitDxfOutline {
                expected_project: input.expected.project_id,
                expected_cursor: input.expected_cursor,
                expected_outline: input.expected.outline.clone(),
                outline: planned.conversion.outline,
            },
            origin,
        )?;
        let applied = DxfOutlineApplied {
            source_sha256: planned.sha256,
            changed: !result.deltas.is_empty(),
            cursor: self.event_cursor(),
            achieved_bound_nm: report.achieved_bound_nm,
            vertices: report.vertices,
            cutouts: report.cutouts,
            result,
        };
        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn regular_file_swapped_for_fifo_refuses_on_the_same_descriptor_without_waiting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("outline.dxf");
        std::fs::write(&path, "0\nEOF\n").unwrap();
        let started = std::time::Instant::now();
        let result = read_dxf_path(PREVIEW_TOOL, &path, || {
            std::fs::remove_file(&path).unwrap();
            rustix::fs::mkfifoat(rustix::fs::CWD, &path, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR).unwrap();
        });
        assert!(result.unwrap_err().to_string().contains("regular file"));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert!(read_dxf_path(PREVIEW_TOOL, directory.path(), || {}).unwrap_err().to_string().contains("regular file"));
        assert!(read_dxf_path(PREVIEW_TOOL, Path::new("/dev/null"), || {}).unwrap_err().to_string().contains("regular file"));
    }

    #[test]
    fn raw_inputs_refuse_duplicates_unknown_fields_and_oversized_values_before_typed_decoding() {
        let ok = r#"{"text":"x","tolerance_nm":10}"#;
        assert!(serde_json::from_str::<PreviewBoardOutlineDxf>(ok).is_ok());
        for bad in [
            r#"{"text":"x","text":"y","tolerance_nm":10}"#,
            r#"{"text":"x","tolerance_nm":10,"tolerance_nm":10}"#,
            r#"{"text":"x","tolerance_nm":10,"extra":1}"#,
            r#"{"text":"x"}"#,
            r#"{"text":"x","tolerance_nm":-1}"#,
        ] {
            assert!(serde_json::from_str::<PreviewBoardOutlineDxf>(bad).is_err(), "{bad}");
        }
        let long_layer = format!(r#"{{"text":"x","layer":"{}","tolerance_nm":10}}"#, "L".repeat(513));
        assert!(serde_json::from_str::<PreviewBoardOutlineDxf>(&long_layer).unwrap_err().to_string().contains("before typed decoding"));
        let long_path = format!(r#"{{"path":"{}","tolerance_nm":10}}"#, "p".repeat(4097));
        assert!(serde_json::from_str::<PreviewBoardOutlineDxf>(&long_path).unwrap_err().to_string().contains("before typed decoding"));
        let points = vec![serde_json::json!({"x":0,"y":0}); 2049];
        let apply = serde_json::json!({"text":"x","tolerance_nm":10,"expected_source_sha256":"0".repeat(64),
            "expected_cursor":{"stream_id":ObjectId::new(),"sequence":0},
            "expected":{"project_id":ObjectId::new(),"outline":{"outer":points,"cutouts":[]}}});
        assert!(preflight_dxf_outline_input(&apply).unwrap_err().contains("2048"));
        let duplicate = r#"{"text":"x","tolerance_nm":10,"expected_source_sha256":"a","expected_source_sha256":"b"}"#;
        assert!(serde_json::from_str::<ApplyBoardOutlineDxf>(duplicate).unwrap_err().to_string().contains("duplicate"));
    }
}
