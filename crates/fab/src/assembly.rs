use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use viamind_doc::{Part, Project, Side};

/// The two generic, deterministic Viamind assembly tables.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyCsv {
    /// Components grouped by exact value and footprint.
    Bom,
    /// Individual component origins in global mm and CCW degrees.
    Placement,
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
/// Unchanged generic assembly request schema, shared by desktop and MCP.
pub fn assembly_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object", "additionalProperties": false,
        "required": ["kind", "path"],
        "properties": {
            "kind": {"type": "string", "enum": ["bom", "placement"]},
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
    _permit: crate::snapshot::Permit,
}
impl AssemblySnapshot {
    /// Validate component/text budgets before copying; rejects busy admission immediately.
    pub fn capture(project: &Project) -> io::Result<Self> {
        let permit = crate::snapshot::Permit::acquire()?;
        if project.board.parts.len() > 100_000 {
            return Err(crate::snapshot::invalid("assembly export supports at most 100000 components"));
        }
        let mut text = crate::snapshot::TextBudget::new();
        for part in project.board.parts.values() {
            for value in [&part.designator, &part.value, &part.footprint] {
                text.add(value)?;
            }
        }
        let parts = project
            .board
            .parts
            .values()
            .map(|p| Part {
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
        Ok(Self { parts, _permit: permit })
    }
    pub fn component_count(&self) -> usize {
        self.parts.len()
    }
    /// Render the unchanged generic CSV format, bounded to32 MiB.
    pub fn csv(&self, kind: AssemblyCsv) -> io::Result<String> {
        render_csv(&self.parts, kind)
    }
    pub fn write(&self, kind: AssemblyCsv, path: &Path, overwrite: bool, native_path: Option<&Path>) -> io::Result<PathBuf> {
        publish_csv(&self.csv(kind)?, path, overwrite, native_path)
    }
}
/// Render a bounded component-only snapshot, preserving text including line breaks.
pub fn csv(project: &Project, kind: AssemblyCsv) -> io::Result<String> {
    AssemblySnapshot::capture(project)?.csv(kind)
}

fn render_csv(parts: &[Part], kind: AssemblyCsv) -> io::Result<String> {
    let mut output = String::new();
    let mut parts: Vec<_> = parts.iter().collect();
    parts.sort_by_key(|part| (&part.designator, part.id));
    match kind {
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

fn row(output: &mut String, fields: &[&str]) -> io::Result<()> {
    let mut bytes = fields.len().saturating_sub(1).saturating_add(2);
    for field in fields {
        bytes = bytes.saturating_add(field.len());
        if field.contains([',', '"', '\r', '\n']) {
            bytes = bytes.saturating_add(2).saturating_add(field.bytes().filter(|&b| b == b'"').count());
        }
    }
    if output.len().saturating_add(bytes) > 32 * 1024 * 1024 {
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
    AssemblySnapshot::capture(project)?.write(kind, path, overwrite, native_path)
}

fn publish_csv(csv: &str, path: &Path, overwrite: bool, native_path: Option<&Path>) -> io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output needs a parent directory"))?;
    if let Some(native) = native_path {
        let destination = path.canonicalize().unwrap_or(
            parent.canonicalize()?.join(path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output needs a file name"))?),
        );
        let native = native.canonicalize().unwrap_or(std::path::absolute(native)?);
        if destination == native {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "CSV output must differ from the active native project path"));
        }
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(csv.as_bytes())?;
    temporary.as_file().sync_all()?;
    if overwrite {
        temporary.persist(&path).map_err(|error| error.error)?;
    } else {
        temporary.persist_noclobber(&path).map_err(|error| error.error)?;
    }
    Ok(path)
}
