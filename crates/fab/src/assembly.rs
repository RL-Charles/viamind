use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use viamind_doc::{Part, Project, Side};

/// Deterministic component inventories and explicitly sourced assembly templates.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyCsv {
    /// Components grouped by exact value and footprint.
    Bom,
    /// Explicit sourcing identifiers and population; includes every component.
    SourcingBom,
    /// Populated components with explicit catalog supplier identifiers.
    CatalogBom,
    /// Populated components with explicit manufacturer part numbers.
    AssemblyBom,
    /// Individual component origins in global mm and CCW degrees.
    Placement,
}
impl AssemblyCsv {
    pub const ALL: [Self; 5] = [Self::Bom, Self::Placement, Self::SourcingBom, Self::CatalogBom, Self::AssemblyBom];
    pub fn name(self) -> &'static str {
        match self {
            Self::Bom => "bom",
            Self::Placement => "placement",
            Self::SourcingBom => "sourcing_bom",
            Self::CatalogBom => "catalog_bom",
            Self::AssemblyBom => "assembly_bom",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Bom => "BOM",
            Self::Placement => "Placement",
            Self::SourcingBom => "Sourcing BOM",
            Self::CatalogBom => "Catalog BOM",
            Self::AssemblyBom => "Assembly BOM",
        }
    }
    fn needs_sourcing(self) -> bool {
        matches!(self, Self::SourcingBom | Self::CatalogBom | Self::AssemblyBom)
    }
    pub(crate) fn populated_only(self) -> bool {
        matches!(self, Self::CatalogBom | Self::AssemblyBom)
    }
}

/// Shared desktop/MCP request; the caller supplies its source cursor type.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, bound(deserialize = "C: Deserialize<'de>"))]
pub struct AssemblyExportRequest<C> {
    pub kind: AssemblyCsv,
    pub path: PathBuf,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub expected_cursor: Option<C>,
}
/// Shared assembly request schema and format catalog for desktop and MCP.
pub fn assembly_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object", "additionalProperties": false,
        "required": ["kind", "path"],
        "properties": {
            "kind": {"type": "string", "enum": AssemblyCsv::ALL.map(AssemblyCsv::name)},
            "path": {"type": "string", "minLength": 1},
            "overwrite": {"type": "boolean", "default": false},
            "expected_cursor": {"type": ["object", "null"], "additionalProperties": false,
                "required": ["stream_id", "sequence"], "properties": {
                    "stream_id": {"type": "string", "format": "uuid"},
                    "sequence": {"type": "integer", "minimum": 0}
                }}
        }
    })
}

/// Immutable component headers; unrelated pads, copper and schematic data are never copied.
pub struct AssemblySnapshot {
    parts: Vec<Part>,
    sourcing: bool,
    _permit: crate::snapshot::Permit,
}
impl AssemblySnapshot {
    /// Validate component/text budgets before copying; rejects busy admission immediately.
    pub fn capture(project: &Project) -> io::Result<Self> {
        Self::capture_for(project, AssemblyCsv::Bom)
    }
    /// Capture only the fields needed by the selected format, after borrowed validation.
    pub fn capture_for(project: &Project, kind: AssemblyCsv) -> io::Result<Self> {
        let permit = crate::snapshot::Permit::acquire()?;
        if project.board.parts.len() > 100_000 {
            return Err(crate::snapshot::invalid("assembly export supports at most 100000 components"));
        }
        let mut text = crate::snapshot::TextBudget::new();
        for part in project.board.parts.values() {
            for value in [&part.designator, &part.value, &part.footprint] {
                text.add(value)?;
            }
            if kind.needs_sourcing()
                && let Some(metadata) = &part.assembly
            {
                metadata.validate().map_err(crate::snapshot::invalid)?;
                for s in metadata.strings() {
                    text.add(s)?;
                }
            }
        }
        // Reference/key allocations and template-specific validation follow the
        // borrowed count/text preflight and precede any component/string copies.
        if kind.populated_only() {
            crate::assembly_templates::validate(project.board.parts.values(), kind)?;
        }
        let parts = project
            .board
            .parts
            .values()
            .map(|p| Part {
                assembly: if kind.needs_sourcing() { p.assembly.clone() } else { None },
                id: p.id,
                designator: p.designator.clone(),
                value: p.value.clone(),
                footprint: p.footprint.clone(),
                position: p.position,
                rotation_mdeg: p.rotation_mdeg,
                side: p.side,
                pads: None,
                schematic_symbol: None,
            })
            .collect();
        Ok(Self { parts, sourcing: kind.needs_sourcing(), _permit: permit })
    }
    pub fn component_count(&self) -> usize {
        self.parts.len()
    }
    /// Template counts describe included components; inventory counts stay unchanged.
    pub fn component_count_for(&self, kind: AssemblyCsv) -> usize {
        if kind.populated_only() {
            self.parts.iter().filter(|part| crate::assembly_templates::populated(part)).count()
        } else {
            self.component_count()
        }
    }
    /// Render the unchanged generic CSV format, bounded to32 MiB.
    pub fn csv(&self, kind: AssemblyCsv) -> io::Result<String> {
        if kind.needs_sourcing() && !self.sourcing {
            return Err(crate::snapshot::invalid("sourcing and template BOMs require a sourcing-aware capture"));
        }
        render_csv(&self.parts, kind)
    }
    pub fn write(&self, kind: AssemblyCsv, path: &Path, overwrite: bool, native_path: Option<&Path>) -> io::Result<PathBuf> {
        stage_csv(&self.csv(kind)?, path, overwrite, native_path)?.publish(native_path)
    }
    /// Render, write and sync outside a document's native file publication guard.
    /// The prepared file retains this snapshot's shared admission permit until publication/drop.
    pub fn prepare(self, kind: AssemblyCsv, path: &Path, overwrite: bool, native_path: Option<&Path>) -> io::Result<PreparedAssemblyCsv> {
        let staged = stage_csv(&self.csv(kind)?, path, overwrite, native_path)?;
        Ok(PreparedAssemblyCsv { staged, _snapshot: self })
    }
}

/// A synced temporary CSV awaiting its small, atomic publication section.
pub struct PreparedAssemblyCsv {
    staged: StagedCsv,
    _snapshot: AssemblySnapshot,
}
impl PreparedAssemblyCsv {
    /// Protect both the captured source native file and the current active native file.
    /// Shared-engine callers must invoke this inside their native lifecycle guard.
    pub fn publish(self, current_native_path: Option<&Path>) -> io::Result<PathBuf> {
        self.staged.publish(current_native_path)
    }
}
/// Render a bounded component-only snapshot, preserving text including line breaks.
pub fn csv(project: &Project, kind: AssemblyCsv) -> io::Result<String> {
    AssemblySnapshot::capture_for(project, kind)?.csv(kind)
}

pub(crate) fn render_csv<'a>(parts: impl IntoIterator<Item = &'a Part>, kind: AssemblyCsv) -> io::Result<String> {
    render_csv_limited(parts, kind, 32 * 1024 * 1024)
}
pub(crate) fn render_csv_limited<'a>(parts: impl IntoIterator<Item = &'a Part>, kind: AssemblyCsv, limit: usize) -> io::Result<String> {
    let limit = limit.min(32 * 1024 * 1024);
    let row = |output: &mut String, fields: &[&str]| row_limited(output, fields, limit);
    let mut output = String::new();
    let mut parts: Vec<_> = parts.into_iter().collect();
    parts.sort_by_key(|part| (&part.designator, part.id));
    match kind {
        AssemblyCsv::CatalogBom | AssemblyCsv::AssemblyBom => {
            crate::assembly_templates::validate(parts.iter().copied(), kind)?;
            return crate::assembly_templates::render(&parts, kind, limit);
        }
        AssemblyCsv::SourcingBom => {
            #[derive(PartialEq, Eq, PartialOrd, Ord)]
            struct Key<'a> {
                value: &'a str,
                footprint: &'a str,
                manufacturer: &'a str,
                mpn: &'a str,
                supplier: &'a str,
                supplier_sku: &'a str,
                populate: bool,
            }
            row(
                &mut output,
                &["Designators", "Component IDs", "Quantity", "Value", "Footprint", "Manufacturer", "MPN", "Supplier", "Supplier SKU", "Population"],
            )?;
            let mut groups: BTreeMap<Key<'_>, Vec<&Part>> = BTreeMap::new();
            for part in parts {
                let metadata = part.assembly.as_ref();
                let key = Key {
                    value: &part.value,
                    footprint: &part.footprint,
                    manufacturer: metadata.and_then(|m| m.manufacturer.as_deref()).unwrap_or(""),
                    mpn: metadata.and_then(|m| m.mpn.as_deref()).unwrap_or(""),
                    supplier: metadata.and_then(|m| m.supplier.as_deref()).unwrap_or(""),
                    supplier_sku: metadata.and_then(|m| m.supplier_sku.as_deref()).unwrap_or(""),
                    populate: metadata.is_none_or(|m| m.populate),
                };
                groups.entry(key).or_default().push(part);
            }
            for (key, parts) in groups {
                let designators = serde_json::to_string(&parts.iter().map(|p| &p.designator).collect::<Vec<_>>()).expect("strings serialize");
                let ids = parts.iter().map(|p| p.id.to_string()).collect::<Vec<_>>().join(" ");
                row(
                    &mut output,
                    &[
                        &designators,
                        &ids,
                        &parts.len().to_string(),
                        key.value,
                        key.footprint,
                        key.manufacturer,
                        key.mpn,
                        key.supplier,
                        key.supplier_sku,
                        if key.populate { "Populate" } else { "Do not populate" },
                    ],
                )?;
            }
        }
        AssemblyCsv::Bom => {
            row(&mut output, &["Designators", "Component IDs", "Quantity", "Value", "Footprint"])?;
            let mut groups: BTreeMap<(&str, &str), Vec<&Part>> = BTreeMap::new();
            for part in parts {
                groups.entry((&part.value, &part.footprint)).or_default().push(part);
            }
            for ((value, footprint), parts) in groups {
                let designators: Vec<_> = parts.iter().map(|part| &part.designator).collect();
                // A JSON array avoids ambiguity in grouped references containing commas/spaces.
                let designators = serde_json::to_string(&designators).expect("strings serialize to JSON");
                let ids = parts.iter().map(|part| part.id.to_string()).collect::<Vec<_>>().join(" ");
                row(&mut output, &[&designators, &ids, &parts.len().to_string(), value, footprint])?;
            }
        }
        AssemblyCsv::Placement => {
            row(&mut output, &["Designator", "Component ID", "Value", "Footprint", "X (mm)", "Y (mm)", "Rotation (deg CCW)", "Side"])?;
            for part in parts {
                row(
                    &mut output,
                    &[
                        &part.designator,
                        &part.id.to_string(),
                        &part.value,
                        &part.footprint,
                        &fixed(part.position.x.0, 6),
                        &fixed(part.position.y.0, 6),
                        &fixed(i64::from(part.rotation_mdeg.rem_euclid(360_000)), 3),
                        match part.side {
                            Side::Top => "Top",
                            Side::Bottom => "Bottom",
                        },
                    ],
                )?;
            }
        }
    }
    Ok(output)
}

// Integer formatting keeps the full i64 nanometre range exact, including MIN.
fn fixed(value: i64, places: u32) -> String {
    let magnitude = i128::from(value).abs();
    let scale = 10_i128.pow(places);
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{}.{:0width$}", magnitude / scale, magnitude % scale, width = places as usize)
}

pub(crate) fn row_limited(output: &mut String, fields: &[&str], limit: usize) -> io::Result<()> {
    let mut bytes = fields.len().saturating_sub(1).saturating_add(2);
    for field in fields {
        bytes = bytes.saturating_add(field.len());
        if field.contains([',', '"', '\r', '\n']) {
            bytes = bytes.saturating_add(2).saturating_add(field.bytes().filter(|&b| b == b'"').count());
        }
    }
    if output.len().saturating_add(bytes) > limit {
        return Err(crate::snapshot::invalid("assembly CSV exceeds 32 MiB"));
    }
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        if field.contains([',', '"', '\r', '\n']) {
            output.push('"');
            output.push_str(&field.replace('"', "\"\""));
            output.push('"');
        } else {
            output.push_str(field);
        }
    }
    output.push_str("\r\n");
    Ok(())
}

/// Atomically publish a CSV, refusing an existing destination by default.
///
/// The parent directory must exist. The returned path is absolute. A temporary
/// file in the same directory prevents partial outputs and permits atomic rename.
/// Supply the active native path to protect it from accidental CSV replacement.
pub fn write_csv(project: &Project, kind: AssemblyCsv, path: &Path, overwrite: bool, native_path: Option<&Path>) -> io::Result<PathBuf> {
    AssemblySnapshot::capture_for(project, kind)?.write(kind, path, overwrite, native_path)
}

struct StagedCsv {
    temporary: tempfile::NamedTempFile,
    path: PathBuf,
    overwrite: bool,
    captured_native: Option<PathBuf>,
}
impl StagedCsv {
    fn publish(self, current_native_path: Option<&Path>) -> io::Result<PathBuf> {
        protect_native(&self.path, self.captured_native.as_deref())?;
        protect_native(&self.path, current_native_path)?;
        if self.overwrite {
            self.temporary.persist(&self.path).map_err(|error| error.error)?;
        } else {
            self.temporary.persist_noclobber(&self.path).map_err(|error| error.error)?;
        }
        Ok(self.path)
    }
}

fn protect_native(path: &Path, native_path: Option<&Path>) -> io::Result<()> {
    if let Some(native) = native_path {
        let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output needs a parent directory"))?;
        let destination = path.canonicalize().unwrap_or(
            parent.canonicalize()?.join(path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output needs a file name"))?),
        );
        let native = native.canonicalize().unwrap_or(std::path::absolute(native)?);
        if destination == native {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "CSV output must differ from the active native project path"));
        }
    }
    Ok(())
}

fn stage_csv(csv: &str, path: &Path, overwrite: bool, native_path: Option<&Path>) -> io::Result<StagedCsv> {
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output needs a parent directory"))?;
    protect_native(&path, native_path)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(csv.as_bytes())?;
    temporary.as_file().sync_all()?;
    Ok(StagedCsv { temporary, path, overwrite, captured_native: native_path.map(Path::to_owned) })
}
