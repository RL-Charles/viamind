//! The native project file: pretty-printed JSON with deterministic order.
//!
//! See `docs/decisions/0001-file-format.md`. A file is an envelope
//! `{ "format": "viamind", "version": 1, "project": { … } }` with 2-space
//! indentation, LF line endings and one trailing newline. Order is structural
//! (field declaration order, maps sorted by id), so saving an unchanged project
//! always produces the same bytes.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ObjectId;
use crate::project::Project;

/// Value of the envelope's `format` field.
pub const FORMAT: &str = "viamind";
/// Highest supported version; legacy projects retain their original schema bytes.
pub const VERSION: u32 = 3;
/// File name suffix for project files, e.g. `blinky.viamind.json`.
pub const EXTENSION: &str = "viamind.json";

/// Why a project file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// Reading or writing the file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The I/O error.
        source: io::Error,
    },
    /// The text is not valid JSON or does not match the model (line and column included).
    #[error("not a valid Viamind project: {0}")]
    Parse(#[from] serde_json::Error),
    /// The envelope's `format` is not `"viamind"`.
    #[error("not a Viamind project file (format is {0:?}, expected \"viamind\")")]
    WrongFormat(String),
    /// The file was written by a newer Viamind.
    #[error("project file version {0} is newer than this build supports (version {VERSION})")]
    UnsupportedVersion(u32),
    /// Geometry is present in an envelope that predates its schema.
    #[error("board outline requires native project version 2 or newer")]
    OutlineVersion,
    /// Electrical pad schema or identity invariant failed.
    #[error("invalid electrical pads: {0}")]
    Pads(String),
    /// A map key does not match the id of the object stored under it.
    #[error("{object} stored under key {key} has id {id}; the key must equal the object's id")]
    KeyMismatch {
        /// Kind of object, e.g. `"part"`.
        object: &'static str,
        /// The map key.
        key: ObjectId,
        /// The object's own id.
        id: ObjectId,
    },
}

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    project: &'a Project,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    format: String,
    version: u32,
    project: Value,
}

/// Serialises `project` as the text of a project file.
pub fn to_string(project: &Project) -> String {
    let envelope = EnvelopeOut { format: FORMAT, version: minimum_version(project), project };
    let mut text = serde_json::to_string_pretty(&envelope).expect("the model always serialises: string keys, no floats");
    text.push('\n');
    text
}

/// Parses the text of a project file, checking the envelope and the model's invariants.
pub fn from_str(text: &str) -> Result<Project, FileError> {
    let envelope: EnvelopeIn = serde_json::from_str(text)?;
    if envelope.format != FORMAT {
        return Err(FileError::WrongFormat(envelope.format));
    }
    if envelope.version > VERSION {
        return Err(FileError::UnsupportedVersion(envelope.version));
    }
    let project: Project = serde_json::from_value(envelope.project)?;
    if project.board.outline.is_some() && envelope.version < 2 {
        return Err(FileError::OutlineVersion);
    }
    validate_pads(&project)?;
    if envelope.version < minimum_version(&project) {
        return Err(FileError::Pads("explicit pads require version 3".into()));
    }
    check_keys("sheet", &project.schematic.sheets, |sheet| sheet.id)?;
    check_keys("part", &project.board.parts, |part| part.id)?;
    Ok(project)
}

fn check_keys<T>(object: &'static str, map: &BTreeMap<ObjectId, T>, id: impl Fn(&T) -> ObjectId) -> Result<(), FileError> {
    match map.iter().find(|(key, value)| **key != id(value)) {
        Some((key, value)) => Err(FileError::KeyMismatch { object, key: *key, id: id(value) }),
        None => Ok(()),
    }
}

/// Writes `project` to `path`. The text goes to a temporary file next to `path`
/// first and is then renamed over it, so a crash never leaves a half-written project.
pub fn save(project: &Project, path: impl AsRef<Path>) -> Result<(), FileError> {
    save_with_overwrite(project, path.as_ref(), true)
}

/// Writes a new native file atomically, preserving an existing destination.
pub fn save_new(project: &Project, path: impl AsRef<Path>) -> Result<(), FileError> {
    save_with_overwrite(project, path.as_ref(), false)
}

fn save_with_overwrite(project: &Project, path: &Path, overwrite: bool) -> Result<(), FileError> {
    let io_err = |source| FileError::Io { path: path.to_owned(), source };
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(io_err)?;
    tmp.write_all(to_string(project).as_bytes()).map_err(io_err)?;
    tmp.as_file().sync_all().map_err(io_err)?;
    let result = if overwrite { tmp.persist(path) } else { tmp.persist_noclobber(path) };
    result.map(drop).map_err(|error| io_err(error.error))
}

/// Reads the project file at `path`.
pub fn load(path: impl AsRef<Path>) -> Result<Project, FileError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|source| FileError::Io { path: path.to_owned(), source })?;
    from_str(&text)
}

/// Minimum native schema that preserves all project features.
pub fn minimum_version(project: &Project) -> u32 {
    if project.board.parts.values().any(|p| p.pads.is_some()) {
        3
    } else if project.board.outline.is_some() {
        2
    } else {
        1
    }
}
/// Validate globally unique pad identities, including collisions with other objects.
pub fn validate_pads(project: &Project) -> Result<(), FileError> {
    let mut ids = std::collections::BTreeSet::from([project.id]);
    ids.extend(project.schematic.sheets.keys().copied());
    ids.extend(project.board.parts.keys().copied());
    for part in project.board.parts.values() {
        if let Some(pads) = &part.pads {
            crate::Pads::try_from(pads.0.clone()).map_err(FileError::Pads)?;
            for pad in &pads.0 {
                if !ids.insert(pad.id) {
                    return Err(FileError::Pads(format!("duplicate object identity {}", pad.id)));
                }
            }
        }
    }
    Ok(())
}
