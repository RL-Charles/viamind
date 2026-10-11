//! Detached rectangular SMD geometry, without project identities or net names.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use viamind_geom::Nm;

/// Maximum footprint file size, including JSON envelope.
pub const MAX_BYTES: usize = 1024 * 1024;

/// Strict local coordinate pair; integer nanometres.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalPosition {
    pub x: Nm,
    pub y: Nm,
}

/// One reusable rectangular SMD pad.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintPad {
    pub number: String,
    pub position: LocalPosition,
    pub width: Nm,
    pub height: Nm,
}

/// Named geometry. Always validate data constructed in memory before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    pub name: String,
    pub pads: Vec<FootprintPad>,
}

impl Footprint {
    pub fn validate(&self) -> Result<(), String> {
        let name_ok = |name: &str| !name.is_empty() && name.trim() == name && name.chars().count() <= 128 && !name.chars().any(char::is_control);
        if !name_ok(&self.name) {
            return Err("footprint name must be trimmed, nonempty, at most 128 characters and contain no controls".into());
        }
        if self.pads.len() > 1024 {
            return Err("at most 1024 pads per footprint".into());
        }
        let mut numbers = BTreeSet::new();
        for p in &self.pads {
            if !name_ok(&p.number) || !numbers.insert(&p.number) {
                return Err("pad numbers must be unique, trimmed, nonempty, at most 128 characters and contain no controls".into());
            }
            if [p.position.x.0, p.position.y.0].iter().any(|n| n.unsigned_abs() > 10_000_000_000)
                || [p.width.0, p.height.0].iter().any(|n| *n <= 0 || *n > 10_000_000_000)
            {
                return Err("pad coordinates must be within ±10000 mm; dimensions greater than zero and at most 10000 mm".into());
            }
        }
        Ok(())
    }

    /// Export only committed electrical geometry in the part's local frame.
    pub fn from_part(part: &crate::Part) -> Result<Self, String> {
        let pads = part.pads.as_ref().ok_or("apply explicit electrical pads before exporting a footprint")?;
        let footprint = Self {
            name: part.footprint.clone(),
            pads: pads
                .0
                .iter()
                .map(|p| FootprintPad {
                    number: p.number.clone(),
                    position: LocalPosition { x: p.position.x, y: p.position.y },
                    width: p.width,
                    height: p.height,
                })
                .collect(),
        };
        footprint.validate()?;
        Ok(footprint)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    footprint: Footprint,
}

/// Parse bounded strict JSON; design-only fields are rejected at every level.
pub fn from_slice(bytes: &[u8]) -> Result<Footprint, String> {
    if bytes.len() > MAX_BYTES {
        return Err("footprint file exceeds 1 MiB".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.format != "viamind-footprint" || envelope.version != 1 {
        return Err("expected viamind-footprint version 1".into());
    }
    envelope.footprint.validate()?;
    Ok(envelope.footprint)
}

pub fn to_vec(footprint: &Footprint) -> Result<Vec<u8>, String> {
    footprint.validate()?;
    let mut bytes = serde_json::to_vec_pretty(&Envelope { format: "viamind-footprint".into(), version: 1, footprint: footprint.clone() })
        .map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_BYTES {
        return Err("footprint file exceeds 1 MiB".into());
    }
    Ok(bytes)
}

pub fn load(path: &Path) -> Result<Footprint, String> {
    if !std::fs::metadata(path).map_err(|e| e.to_string())?.is_file() {
        return Err("footprint path must be a regular file".into());
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("footprint path must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    from_slice(&bytes)
}

/// Atomically publish a new library file; existing entries are always preserved.
pub fn save_new(footprint: &Footprint, path: &Path, active_project: Option<&Path>) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("choose a footprint file path".into());
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    let target = parent.join(path.file_name().ok_or("choose a file name")?);
    if let Some(active) = active_project {
        let active = active.canonicalize().unwrap_or_else(|_| active.to_owned());
        if target == active || target.canonicalize().is_ok_and(|p| p == active) {
            return Err("cannot replace the active native project with a footprint".into());
        }
    }
    let bytes = to_vec(footprint)?;
    let mut temp = tempfile::NamedTempFile::new_in(&parent).map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(&target).map_err(|e| format!("{}: {}", target.display(), e.error))?;
    Ok(target)
}
