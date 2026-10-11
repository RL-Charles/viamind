//! Deterministic, aligned outer-copper Gerber X2 and XNC exports.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    io::{self, Write},
    path::{Path, PathBuf},
};
use viamind_doc::{CopperLayer, Project, Side};
use viamind_geom::Point;

const MAX_OBJECTS: usize = 200_000;
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_COORD: i64 = 99_999_999_999;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FabricationOptions {
    /// Applied to all explicit pads and untented vias, on both outer sides.
    #[serde(default)]
    pub mask_expansion_mm: f64,
}

/// The schema shared by desktop and MCP snapshot export actions.
pub fn fabrication_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{
        "path":{"type":"string","minLength":1},
        "mask_expansion_mm":{"type":"number","minimum":0,"maximum":10,"default":0},
        "expected_cursor":{"type":["object","null"],"additionalProperties":false,"required":["stream_id","sequence"],"properties":{
            "stream_id":{"type":"string","format":"uuid"},"sequence":{"type":"integer","minimum":0}
        }}
    }})
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FabricationManifest {
    pub project_id: String,
    pub units: String,
    pub coordinate_system: String,
    pub mask_expansion_nm: i64,
    pub smd_pad_count: usize,
    pub track_count: usize,
    pub through_via_count: usize,
    pub cutout_count: usize,
    pub files: Vec<String>,
    pub exclusions: Vec<String>,
}

/// Fully rendered, bounded snapshot package, before filesystem publication.
pub struct FabricationPackage {
    pub files: BTreeMap<String, String>,
    pub manifest: FabricationManifest,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn decimal(value: i64, places: u32) -> String {
    let magnitude = i128::from(value).abs();
    let scale = 10_i128.pow(places);
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{}.{:0width$}", magnitude / scale, magnitude % scale, width = places as usize)
}
fn coordinate(x: i64, y: i64) -> io::Result<(i64, i64)> {
    if x.unsigned_abs() > MAX_COORD as u64 || y.unsigned_abs() > MAX_COORD as u64 {
        return Err(invalid("fabrication coordinates must fit ±99999.999999 mm"));
    }
    Ok((x, -y))
}
fn point(p: Point) -> io::Result<(i64, i64)> {
    coordinate(p.x.0, p.y.0)
}
fn rotated_center(x_mm: f64, y_mm: f64) -> io::Result<(i64, i64)> {
    if !x_mm.is_finite() || !y_mm.is_finite() || x_mm.abs() >= 100_000.0 || y_mm.abs() >= 100_000.0 {
        return Err(invalid("rotated pad center exceeds fabrication coordinate range"));
    }
    coordinate((x_mm * 1_000_000.0).round() as i64, (y_mm * 1_000_000.0).round() as i64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Aperture {
    width: i64,
    height: Option<i64>,
    function: &'static str,
}
#[derive(Clone, Copy)]
struct Graphic {
    aperture: Aperture,
    start: (i64, i64),
    end: Option<(i64, i64)>,
    rotation: i32,
}
fn circle(width: i64, function: &'static str) -> Aperture {
    Aperture { width, height: None, function }
}
fn flash(aperture: Aperture, start: (i64, i64), rotation: i32) -> Graphic {
    Graphic { aperture, start, end: None, rotation }
}

fn gerber(graphics: &[Graphic], function: &str, negative: bool, project_id: &str) -> String {
    let mut output = format!(
        "G04 Viamind scoped outer-copper snapshot*\n%FSLAX56Y56*%\n%MOMM*%\n%TF.Part,Single*%\n%TF.FileFunction,{function}*%\n%TF.FilePolarity,{}*%\n%TF.SameCoordinates,{project_id}*%\n",
        if negative { "Negative" } else { "Positive" }
    );
    let unique: BTreeSet<_> = graphics.iter().map(|g| g.aperture).collect();
    let apertures: BTreeMap<_, _> = unique.into_iter().enumerate().map(|(index, a)| (a, index + 10)).collect();
    for (a, number) in &apertures {
        if !a.function.is_empty() {
            writeln!(output, "%TA.AperFunction,{}*%", a.function).unwrap();
        }
        if let Some(height) = a.height {
            writeln!(output, "%ADD{number}R,{}X{}*%", decimal(a.width, 6), decimal(height, 6)).unwrap();
        } else {
            writeln!(output, "%ADD{number}C,{}*%", decimal(a.width, 6)).unwrap();
        }
        output.push_str("%TD*%\n");
    }
    output.push_str("%LPD*%\n%LMN*%\n%LS1*%\nG01*\n");
    for g in graphics {
        writeln!(output, "D{}*\n%LR{}*%", apertures[&g.aperture], decimal(i64::from(g.rotation.rem_euclid(360_000)), 3)).unwrap();
        let (x, y) = g.start;
        if let Some((ex, ey)) = g.end {
            writeln!(output, "X{x}Y{y}D02*\nX{ex}Y{ey}D01*").unwrap();
        } else {
            writeln!(output, "X{x}Y{y}D03*").unwrap();
        }
    }
    output.push_str("M02*\n");
    output
}

/// Render supported, explicit model geometry. This is not a fabrication certification.
pub fn fabrication(project: &Project, options: FabricationOptions) -> io::Result<FabricationPackage> {
    if !options.mask_expansion_mm.is_finite() || !(0.0..=10.0).contains(&options.mask_expansion_mm) {
        return Err(invalid("mask_expansion_mm must be finite and between 0 and 10 mm"));
    }
    let outline = project.board.outline.as_ref().ok_or_else(|| invalid("fabrication export requires an explicit board outline"))?;
    let mut pads = 0usize;
    for part in project.board.parts.values() {
        let explicit = part.pads.as_ref().ok_or_else(|| {
            invalid(format!("component {} has no explicit pads; legacy graphical footprint previews cannot be fabricated", part.designator))
        })?;
        pads = pads.saturating_add(explicit.0.len());
    }
    let objects = pads
        .saturating_mul(2)
        .saturating_add(project.board.tracks.len())
        .saturating_add(project.board.vias.len().saturating_mul(5))
        .saturating_add(outline.rings().map(|r| r.points.len()).sum::<usize>());
    if objects > MAX_OBJECTS {
        return Err(invalid("fabrication export supports at most 200000 primitives"));
    }
    outline.validate().map_err(|e| invalid(e.to_string()))?;
    viamind_doc::file::validate_pads(project).map_err(|e| invalid(e.to_string()))?;
    viamind_doc::file::validate_copper(project).map_err(|e| invalid(e.to_string()))?;
    let expansion = (options.mask_expansion_mm * 1_000_000.0).round() as i64;
    let mut copper = [Vec::new(), Vec::new()];
    let mut mask = [Vec::new(), Vec::new()];
    for part in project.board.parts.values() {
        let side = usize::from(part.side == Side::Bottom);
        for pad in &part.pads.as_ref().unwrap().0 {
            let (x, y) = part.local_to_world_mm(pad.position.x.to_mm(), pad.position.y.to_mm());
            let center = rotated_center(x, y)?;
            let aperture = Aperture { width: pad.width.0, height: Some(pad.height.0), function: "SMDPad,CuDef" };
            copper[side].push(flash(aperture, center, part.rotation_mdeg));
            mask[side].push(flash(
                Aperture { width: aperture.width + 2 * expansion, height: Some(pad.height.0 + 2 * expansion), function: "" },
                center,
                part.rotation_mdeg,
            ));
        }
    }
    for track in project.board.tracks.values() {
        let side = usize::from(track.layer == CopperLayer::Bottom);
        copper[side].push(Graphic {
            aperture: circle(track.width.0, "Conductor"),
            start: point(track.start)?,
            end: Some(point(track.end)?),
            rotation: 0,
        });
    }
    let mut drills: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    for via in project.board.vias.values() {
        let center = point(via.position)?;
        for side in 0..2 {
            copper[side].push(flash(circle(via.diameter.0, "ViaPad"), center, 0));
            mask[side].push(flash(circle(via.diameter.0 + 2 * expansion, ""), center, 0));
        }
        drills.entry(via.drill.0).or_default().push(center);
    }
    let mut profile = Vec::new();
    for ring in outline.rings() {
        for edge in ring.edges() {
            profile.push(Graphic { aperture: circle(0, "Profile"), start: point(edge.a)?, end: Some(point(edge.b)?), rotation: 0 });
        }
    }
    let id = project.id.to_string();
    let mut files = BTreeMap::new();
    for (name, function, negative, graphics) in [
        ("top-copper.gbr", "Copper,L1,Top", false, &copper[0]),
        ("bottom-copper.gbr", "Copper,L2,Bot", false, &copper[1]),
        ("top-mask.gbr", "Soldermask,Top", true, &mask[0]),
        ("bottom-mask.gbr", "Soldermask,Bot", true, &mask[1]),
        ("profile.gbr", "Profile,NP", false, &profile),
    ] {
        files.insert(name.to_owned(), gerber(graphics, function, negative, &id));
    }
    let mut drill = String::from("; Viamind plated through vias, finished diameters, aligned top view\nM48\nMETRIC\n");
    for (i, (diameter, _)) in drills.iter().enumerate() {
        writeln!(drill, "T{}C{}", i + 1, decimal(*diameter, 6)).unwrap();
    }
    drill.push_str("%\nG05\n");
    for (i, (_, positions)) in drills.iter().enumerate() {
        writeln!(drill, "T{}", i + 1).unwrap();
        for &(x, y) in positions {
            writeln!(drill, "X{}Y{}", decimal(x, 6), decimal(y, 6)).unwrap();
        }
    }
    drill.push_str("M30\n");
    files.insert("plated-through.xnc".into(), drill);
    let manifest=FabricationManifest { project_id:id,units:"mm (Gerber 5.6, 1 nm; XNC explicit decimals)".into(),coordinate_system:"Model origin; X right; Y up (-model Y); top view for every layer; bottom files are not mirrored".into(),mask_expansion_nm:expansion,smd_pad_count:pads,track_count:project.board.tracks.len(),through_via_count:project.board.vias.len(),cutout_count:outline.cutouts.len(),files:files.keys().cloned().chain(std::iter::once("manifest.json".into())).collect(),exclusions:vec!["Scoped geometry export, not fabrication readiness or clearance/continuity certification".into(),"No legacy previews, inner copper, pours, arcs, component through holes/slots, blind/buried/microvias, paste, silkscreen, netlist/X3 assembly or material/finish/stackup instructions".into(),"Mask expansion is an export parameter; all vias untented; no manufacturer rules inferred".into()] };
    files.insert("manifest.json".into(), serde_json::to_string_pretty(&manifest).map_err(io::Error::other)? + "\n");
    if files.values().map(String::len).sum::<usize>() > MAX_BYTES {
        return Err(invalid("fabrication export exceeds 128 MiB"));
    }
    Ok(FabricationPackage { files, manifest })
}

/// Publish a new directory atomically on Linux. Existing destinations always reject.
pub fn write_fabrication(
    project: &Project,
    options: FabricationOptions,
    path: &Path,
    native: Option<&Path>,
) -> io::Result<(PathBuf, FabricationManifest)> {
    if path.as_os_str().is_empty() {
        return Err(invalid("fabrication output directory must not be empty"));
    }
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or_else(|| invalid("output directory needs a parent"))?;
    let name = path.file_name().ok_or_else(|| invalid("output directory needs a name"))?;
    let canonical = parent.canonicalize()?.join(name);
    if let Some(native) = native {
        let native = native.canonicalize().unwrap_or(std::path::absolute(native)?);
        if canonical == native || path.canonicalize().is_ok_and(|p| p == native) {
            return Err(invalid("fabrication output must differ from active native project path"));
        }
    }
    let package = fabrication(project, options)?;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = package;
        Err(io::Error::new(io::ErrorKind::Unsupported, "atomic fabrication directory export currently requires Linux"))
    }
    #[cfg(target_os = "linux")]
    {
        let stage = tempfile::Builder::new().prefix(".viamind-fabrication-").tempdir_in(parent)?;
        for (name, contents) in &package.files {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(stage.path().join(name))?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::File::open(stage.path())?.sync_all()?;
        rustix::fs::renameat_with(rustix::fs::CWD, stage.path(), rustix::fs::CWD, &path, rustix::fs::RenameFlags::NOREPLACE)?;
        Ok((path, package.manifest))
    }
}
