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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apertures: Option<crate::PadApertures>,
}

/// Named geometry. Always validate data constructed in memory before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<Box<crate::FootprintArtwork>>,
    pub name: String,
    pub pads: Vec<FootprintPad>,
}

impl Footprint {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(artwork) = &self.artwork {
            artwork.validate()?;
        }
        let name_ok = |name: &str| !name.is_empty() && name.trim() == name && name.chars().count() <= 128 && !name.chars().any(char::is_control);
        if !name_ok(&self.name) {
            return Err("footprint name must be trimmed, nonempty, at most 128 characters and contain no controls".into());
        }
        if self.pads.len() > 1024 {
            return Err("at most 1024 pads per footprint".into());
        }
        let mut numbers = BTreeSet::new();
        for p in &self.pads {
            if let Some(apertures) = &p.apertures {
                apertures.validate_at(viamind_geom::Point { x: p.position.x, y: p.position.y })?;
            }
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

    /// Export committed pads and explicit artwork in the part's local frame.
    pub fn from_part(part: &crate::Part) -> Result<Self, String> {
        if let Some(artwork) = &part.artwork {
            artwork.validate()?;
        }
        let pads = part.pads.as_ref().ok_or("apply explicit electrical pads before exporting a footprint")?;
        let footprint = Self {
            artwork: part.artwork.clone(),
            name: part.footprint.clone(),
            pads: pads
                .0
                .iter()
                .map(|p| FootprintPad {
                    number: p.number.clone(),
                    position: LocalPosition { x: p.position.x, y: p.position.y },
                    width: p.width,
                    height: p.height,
                    apertures: p.apertures.clone(),
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
    let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let version = raw.get("version").and_then(serde_json::Value::as_u64);
    let artwork = raw.get("footprint").and_then(|f| f.get("artwork"));
    let pads = raw.get("footprint").and_then(|f| f.get("pads")).and_then(serde_json::Value::as_array);
    if pads.is_some_and(|p| p.len() > 1024) {
        return Err("at most1024 footprint pads before typed decoding".into());
    }
    let has_aperture_member = pads.is_some_and(|p| p.iter().any(|p| p.get("apertures").is_some()));
    let has_aperture_policy = pads.is_some_and(|p| p.iter().any(|p| p.get("apertures").is_some_and(serde_json::Value::is_object)));
    if matches!(version, Some(1 | 2)) && has_aperture_member {
        return Err("library1/2 forbid any apertures member, including null".into());
    }
    if version == Some(3) && !has_aperture_policy {
        return Err("library3 requires at least one non-null pad apertures policy".into());
    }
    if (version == Some(1) && artwork.is_some())
        || (version == Some(2) && !artwork.is_some_and(serde_json::Value::is_object))
        || (version == Some(3) && artwork.is_some_and(|a| !a.is_object()))
    {
        return Err("library1 requires artwork missing; library2 requires non-null artwork object".into());
    }
    // Decode the original bounded bytes so duplicate fields remain visible to serde.
    // The presence preflight must never become a lossy Value-to-typed roundtrip.
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.format != "viamind-footprint" || !matches!(envelope.version, 1 | 2 | 3) {
        return Err("expected viamind-footprint version1,2 or3".into());
    }
    envelope.footprint.validate()?;
    Ok(envelope.footprint)
}

pub fn to_vec(footprint: &Footprint) -> Result<Vec<u8>, String> {
    footprint.validate()?;
    let mut bytes = serde_json::to_vec_pretty(&Envelope {
        format: "viamind-footprint".into(),
        version: if footprint.pads.iter().any(|p| p.apertures.is_some()) {
            3
        } else if footprint.artwork.is_some() {
            2
        } else {
            1
        },
        footprint: footprint.clone(),
    })
    .map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_BYTES {
        return Err("footprint file exceeds 1 MiB".into());
    }
    Ok(bytes)
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err("choose a nonempty footprint path at most 4096 encoded bytes".into());
    }
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    if absolute.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err("absolute footprint path exceeds 4096 encoded bytes".into());
    }
    Ok(absolute)
}

pub fn load(path: &Path) -> Result<Footprint, String> {
    load_with_before_open(path, || {})
}
fn load_with_before_open(path: &Path, before_open: impl FnOnce()) -> Result<Footprint, String> {
    let path = absolute_path(path)?;
    let before = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !before.is_file() || before.len() > MAX_BYTES as u64 {
        return Err("footprint path must be a regular file at most 1 MiB".into());
    }
    before_open();
    let fd =
        rustix::fs::open(&path, rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC, rustix::fs::Mode::empty())
            .map_err(|e| e.to_string())?;
    let file = std::fs::File::from(fd);
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file() || opened.len() > MAX_BYTES as u64 {
        return Err("opened footprint must be a regular file at most 1 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let after = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if bytes.len() as u64 != opened.len() || !after.is_file() || after.len() != opened.len() || after.modified().ok() != opened.modified().ok() {
        return Err("footprint file changed while reading; reload".into());
    }
    from_slice(&bytes)
}

/// Bounded synced output, staged before acquiring the native lifecycle guard.
pub struct StagedFootprintFile {
    temporary: tempfile::NamedTempFile,
    path: PathBuf,
}
impl StagedFootprintFile {
    /// Atomically publish only a new file, with the current active native path protected.
    pub fn publish(self, active_project: Option<&Path>) -> Result<PathBuf, String> {
        if let Some(active) = active_project {
            let active = active.canonicalize().unwrap_or_else(|_| active.to_owned());
            if self.path == active || self.path.canonicalize().is_ok_and(|p| p == active) {
                return Err("cannot replace the active native project with a footprint".into());
            }
        }
        self.temporary.persist_noclobber(&self.path).map_err(|e| format!("{}: {}", self.path.display(), e.error))?;
        Ok(self.path)
    }
}

/// Serialize/sync bounded native geometry without publishing any destination yet.
pub fn stage_new(footprint: &Footprint, path: &Path) -> Result<StagedFootprintFile, String> {
    let path = absolute_path(path)?;
    let parent = path.parent().ok_or("choose a footprint parent directory")?.canonicalize().map_err(|e| e.to_string())?;
    let target = parent.join(path.file_name().ok_or("choose a file name")?);
    if target.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err("resolved footprint path exceeds 4096 encoded bytes".into());
    }
    let bytes = to_vec(footprint)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&parent).map_err(|e| e.to_string())?;
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    Ok(StagedFootprintFile { temporary, path: target })
}

/// Atomically publish a new library file; existing entries are always preserved.
pub fn save_new(footprint: &Footprint, path: &Path, active_project: Option<&Path>) -> Result<PathBuf, String> {
    stage_new(footprint, path)?.publish(active_project)
}

#[cfg(test)]
mod io_tests {
    use super::*;
    fn footprint() -> Footprint {
        Footprint { artwork: None, name: "Original".into(), pads: Vec::new() }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn raced_fifo_native_library_open_never_waits_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library");
        std::fs::write(&path, to_vec(&footprint()).unwrap()).unwrap();
        let error = load_with_before_open(&path, || {
            std::fs::remove_file(&path).unwrap();
            assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
        })
        .unwrap_err();
        assert!(error.contains("regular file"));
    }
    #[test]
    fn staged_publication_refuses_native_paths_conflicts_and_symlinks_without_changing_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new-library.json");
        let staged = stage_new(&footprint(), &path).unwrap();
        assert!(!path.exists());
        assert!(staged.publish(Some(&path)).unwrap_err().contains("native project"));
        assert!(!path.exists());
        let bytes = b"existing destination";
        let staged = stage_new(&footprint(), &path).unwrap();
        std::fs::write(&path, bytes).unwrap();
        assert!(staged.publish(None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        #[cfg(unix)]
        {
            let alias = directory.path().join("alias.json");
            std::os::unix::fs::symlink(&path, &alias).unwrap();
            assert!(save_new(&footprint(), &alias, None).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let too_long = PathBuf::from("x".repeat(4097));
        assert!(load(&too_long).unwrap_err().contains("4096"));
        assert!(save_new(&footprint(), &too_long, None).is_err());
    }
}
