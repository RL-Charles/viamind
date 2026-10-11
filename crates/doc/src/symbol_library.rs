//! Detached single-unit rectangular symbol definitions, without instance identities.
use crate::{
    ObjectId,
    schematic::{Coordinate, PinType, SchematicSymbol, SymbolPin},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use viamind_geom::Nm;

pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
pub const MAX_PINS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplatePin {
    pub number: String,
    pub name: String,
    pub position: Coordinate,
    pub electrical: PinType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolTemplate {
    pub name: String,
    pub value: String,
    pub footprint: Option<String>,
    pub width: Nm,
    pub height: Nm,
    pub pins: Vec<TemplatePin>,
}

fn text_budget<'a>(pins: usize, fields: impl Iterator<Item = &'a str>) -> Result<(), String> {
    if pins > MAX_PINS {
        return Err("at most 1024 pins per symbol template".into());
    }
    let mut total = 0usize;
    for s in fields {
        total = total.saturating_add(s.len());
        if s.len() > 4096 || total > MAX_TEXT_BYTES {
            return Err("symbol template text exceeds 4096 bytes per field or 256 KiB aggregate".into());
        }
    }
    Ok(())
}

impl SymbolTemplate {
    pub fn validate(&self) -> Result<(), String> {
        text_budget(
            self.pins.len(),
            [self.name.as_str(), &self.value, self.footprint.as_deref().unwrap_or("")]
                .into_iter()
                .chain(self.pins.iter().flat_map(|p| [p.number.as_str(), p.name.as_str()])),
        )?;
        crate::schematic::name(&self.name, false, 128)?;
        crate::schematic::name(&self.value, true, 1024)?;
        if let Some(footprint) = &self.footprint {
            crate::schematic::name(footprint, false, 128)?;
        }
        if [self.width.0, self.height.0].iter().any(|n| *n <= 0 || *n > 10_000_000_000) {
            return Err("symbol template dimensions must be positive and at most 10000 mm".into());
        }
        let mut numbers = BTreeSet::new();
        for pin in &self.pins {
            crate::schematic::name(&pin.number, false, 128)?;
            crate::schematic::name(&pin.name, true, 128)?;
            pin.position.validate()?;
            if !numbers.insert(&pin.number) {
                return Err("symbol template pin numbers must be unique".into());
            }
        }
        Ok(())
    }

    /// Copy only committed local definitions after checking every copied field.
    pub fn from_symbol(name: &str, symbol: &SchematicSymbol) -> Result<Self, String> {
        text_budget(
            symbol.pins.len(),
            [name, symbol.value.as_str(), symbol.footprint.as_deref().unwrap_or("")]
                .into_iter()
                .chain(symbol.pins.iter().flat_map(|p| [p.number.as_str(), p.name.as_str()])),
        )?;
        let template = Self {
            name: name.into(),
            value: symbol.value.clone(),
            footprint: symbol.footprint.clone(),
            width: symbol.width,
            height: symbol.height,
            pins: symbol
                .pins
                .iter()
                .map(|p| TemplatePin { number: p.number.clone(), name: p.name.clone(), position: p.position, electrical: p.electrical })
                .collect(),
        };
        template.validate()?;
        Ok(template)
    }

    /// Always creates detached instance and pin identities. Existing instances are never updated.
    pub fn instantiate(&self, designator: &str, position: Coordinate, rotation_quarters: u8, mirrored: bool) -> Result<SchematicSymbol, String> {
        self.validate()?;
        crate::schematic::name(designator, false, 128)?;
        position.validate()?;
        if rotation_quarters > 3 {
            return Err("rotation_quarters must be within 0..3".into());
        }
        // Doubled integer corners preserve odd-Nm dimensions without rounding.
        for dx in [-self.width.0, self.width.0] {
            for dy in [-self.height.0, self.height.0] {
                let (mut x, mut y) = (i128::from(dx), i128::from(dy));
                if mirrored {
                    x = -x;
                }
                for _ in 0..rotation_quarters {
                    (x, y) = (y, -x);
                }
                if [x + 2 * i128::from(position.x.0), y + 2 * i128::from(position.y.0)].iter().any(|n| n.abs() > 20_000_000_000) {
                    return Err("placed symbol body must remain within ±10000 mm".into());
                }
            }
        }
        // Check transformed pin positions before copying strings or generating IDs.
        let transform = SchematicSymbol {
            id: ObjectId::from_uuid(uuid::Uuid::nil()),
            designator: String::new(),
            value: String::new(),
            footprint: None,
            position,
            rotation_quarters,
            mirrored,
            width: self.width,
            height: self.height,
            pins: vec![],
        };
        for p in &self.pins {
            transform.pin_position(p.position)?;
        }
        let symbol = SchematicSymbol {
            id: ObjectId::new(),
            designator: designator.into(),
            value: self.value.clone(),
            footprint: self.footprint.clone(),
            position,
            rotation_quarters,
            mirrored,
            width: self.width,
            height: self.height,
            pins: self
                .pins
                .iter()
                .map(|p| SymbolPin {
                    id: ObjectId::new(),
                    number: p.number.clone(),
                    name: p.name.clone(),
                    position: p.position,
                    electrical: p.electrical,
                })
                .collect(),
        };
        symbol.validate()?;
        Ok(symbol)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    symbol: SymbolTemplate,
}
pub fn from_slice(bytes: &[u8]) -> Result<SymbolTemplate, String> {
    if bytes.len() > MAX_BYTES {
        return Err("symbol template file exceeds 1 MiB".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.format != "viamind-symbol" || envelope.version != 1 {
        return Err("expected viamind-symbol version 1".into());
    }
    envelope.symbol.validate()?;
    Ok(envelope.symbol)
}
pub fn to_vec(template: &SymbolTemplate) -> Result<Vec<u8>, String> {
    template.validate()?;
    #[derive(Serialize)]
    struct Output<'a> {
        format: &'static str,
        version: u32,
        symbol: &'a SymbolTemplate,
    }
    let mut bytes = serde_json::to_vec_pretty(&Output { format: "viamind-symbol", version: 1, symbol: template }).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_BYTES {
        return Err("symbol template file exceeds 1 MiB".into());
    }
    Ok(bytes)
}
fn check_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err("choose a symbol file path of 1..4096 bytes".into());
    }
    Ok(())
}
pub fn load(path: &Path) -> Result<SymbolTemplate, String> {
    check_path(path)?;
    // Inspect the opened descriptor, without waiting for a writer if the path
    // names (or is concurrently replaced by) a FIFO.
    #[cfg(unix)]
    let file = std::fs::File::from(
        rustix::fs::open(path, rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC, rustix::fs::Mode::empty())
            .map_err(|e| format!("{}: {e}", path.display()))?,
    );
    #[cfg(not(unix))]
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("symbol template path must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    from_slice(&bytes)
}

#[cfg(all(test, unix))]
mod regular_file_tests {
    #[test]
    fn loading_a_fifo_without_a_writer_rejects_instead_of_waiting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("replaced.viamind-symbol.json");
        rustix::fs::mkfifoat(rustix::fs::CWD, &path, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR).unwrap();
        assert!(super::load(&path).unwrap_err().contains("regular file"));
    }
}
/// Bounded synchronized symbol bytes awaiting NEW publication.
pub struct StagedSymbolFile {
    temporary: tempfile::NamedTempFile,
    path: PathBuf,
}
impl StagedSymbolFile {
    /// Publish without replacing an existing destination, protecting the current native path.
    pub fn publish(self, native: Option<&Path>) -> Result<PathBuf, String> {
        if let Some(native) = native {
            let native = native.canonicalize().unwrap_or_else(|_| native.to_owned());
            if self.path == native || self.path.canonicalize().is_ok_and(|p| p == native) {
                return Err("cannot replace the active native project with a symbol template".into());
            }
        }
        self.temporary.persist_noclobber(&self.path).map_err(|e| format!("{}: {}", self.path.display(), e.error))?;
        Ok(self.path)
    }
}

/// Validate, serialize and synchronize a private sibling without publishing the destination.
pub fn stage_new(template: &SymbolTemplate, path: &Path) -> Result<StagedSymbolFile, String> {
    check_path(path)?;
    stage_bytes_new(&to_vec(template)?, path)
}

/// Share synchronized NEW publication with other validated bounded native symbol formats.
pub(crate) fn stage_bytes_new(bytes: &[u8], path: &Path) -> Result<StagedSymbolFile, String> {
    if bytes.len() > MAX_BYTES {
        return Err("symbol file exceeds 1 MiB".into());
    }
    check_path(path)?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).canonicalize().map_err(|e| e.to_string())?;
    let target = parent.join(path.file_name().ok_or("choose a symbol file name")?);
    if target.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err("resolved symbol path exceeds 4096 encoded bytes".into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    Ok(StagedSymbolFile { temporary, path: target })
}

/// Publish only a NEW library entry. Engine callers supply the current native lifecycle guard.
pub fn save_new(template: &SymbolTemplate, path: &Path, native: Option<&Path>) -> Result<PathBuf, String> {
    stage_new(template, path)?.publish(native)
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    fn template() -> SymbolTemplate {
        SymbolTemplate {
            name: "Original odd geometry".into(),
            value: "Exact".into(),
            footprint: None,
            width: Nm(1_000_001),
            height: Nm(2_000_003),
            pins: vec![TemplatePin {
                number: "1".into(),
                name: "IN".into(),
                position: Coordinate { x: Nm(3), y: Nm(-5) },
                electrical: PinType::Input,
            }],
        }
    }
    #[test]
    fn private_staging_exact_publication_drop_and_conflicts_preserve_destinations() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("definition.json");
        let definition = template();
        let staged = stage_new(&definition, &path).unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        drop(staged);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        let staged = stage_new(&definition, &path).unwrap();
        assert!(staged.publish(Some(&path)).unwrap_err().contains("active native"));
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        let staged = stage_new(&definition, &path).unwrap();
        assert_eq!(staged.publish(None).unwrap(), path);
        assert_eq!(std::fs::read(&path).unwrap(), to_vec(&definition).unwrap());
        assert_eq!(load(&path).unwrap(), definition);
        assert!(save_new(&definition, &path, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), to_vec(&definition).unwrap());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    #[test]
    fn destination_created_after_stage_and_symlink_alias_are_never_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("racing.json");
        let staged = stage_new(&template(), &path).unwrap();
        std::fs::write(&path, b"peer bytes").unwrap();
        assert!(staged.publish(None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"peer bytes");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            let alias = directory.path().join("alias.json");
            std::os::unix::fs::symlink(&path, &alias).unwrap();
            assert!(stage_new(&template(), &alias).unwrap().publish(Some(&path)).unwrap_err().contains("active native"));
            assert!(save_new(&template(), &alias, None).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"peer bytes");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
        }
        assert!(stage_new(&template(), &PathBuf::from("x".repeat(4097))).err().unwrap().contains("4096"));
    }
}
