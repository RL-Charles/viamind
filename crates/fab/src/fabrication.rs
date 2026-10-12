//! Deterministic, aligned outer-copper Gerber X2 and XNC exports.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    io::{self, Write},
    path::{Path, PathBuf},
};
use viamind_doc::{ApertureKind, ApertureResolution, CopperLayer, Project, Side, resolve_aperture};
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_silkscreen: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_paste: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_npth: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Shared desktop/MCP request. Cursor types belong to the caller, not the document.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, bound(deserialize = "C: Deserialize<'de>"))]
pub struct FabricationExportRequest<C> {
    pub path: PathBuf,
    #[serde(default)]
    pub mask_expansion_mm: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_silkscreen: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_paste: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_npth: bool,
    #[serde(default)]
    pub expected_cursor: Option<C>,
}
impl<C> FabricationExportRequest<C> {
    /// Cheap input checks before snapshot budget/admission/copy work.
    pub fn validate(&self) -> io::Result<()> {
        if self.path.as_os_str().is_empty() {
            return Err(invalid("path must not be empty"));
        }
        if !self.mask_expansion_mm.is_finite() || !(0.0..=10.0).contains(&self.mask_expansion_mm) {
            return Err(invalid("mask expansion needs a finite number from 0 to 10 mm"));
        }
        Ok(())
    }
}

/// The schema shared by desktop and MCP snapshot export actions.
pub fn fabrication_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{
        "path":{"type":"string","minLength":1},
        "mask_expansion_mm":{"type":"number","minimum":0,"maximum":10,"default":0},
        "include_silkscreen":{"type":"boolean","default":false},
        "include_paste":{"type":"boolean","default":false},
        "include_npth":{"type":"boolean","default":false},
        "expected_cursor":{"type":["object","null"],"additionalProperties":false,"required":["stream_id","sequence"],"properties":{
            "stream_id":{"type":"string","format":"uuid"},"sequence":{"type":"integer","minimum":0}
        }}
    }})
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FabricationManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonplated_holes: Option<NonplatedHoleSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad_apertures: Option<PadApertureSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint_artwork: Option<FootprintArtworkSummary>,
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

/// Mechanical intent counts; absent data is not an inspection or clearance certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonplatedHoleSummary {
    pub include_npth: bool,
    pub authored_components: usize,
    pub explicit_empty_components: usize,
    pub physical_drills: usize,
    pub tool_diameters: usize,
}

/// Explicit opening coverage; no stencil or manufacturer suitability claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadApertureSummary {
    pub include_paste: bool,
    pub explicit_pads: usize,
    pub legacy_pads: usize,
    pub mask_rectangles: usize,
    pub mask_omissions: usize,
    pub paste_rectangles: usize,
    pub paste_omissions: usize,
    pub top_mask_rectangles: usize,
    pub bottom_mask_rectangles: usize,
    pub top_paste_rectangles: usize,
    pub bottom_paste_rectangles: usize,
    pub explicit_coverage_complete: bool,
}

/// Captured explicit artwork counts; no clipping or courtyard checking is certified.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintArtworkSummary {
    pub include_silkscreen: bool,
    pub explicit_components: usize,
    pub top_silkscreen_segments: usize,
    pub bottom_silkscreen_segments: usize,
    pub courtyard_count: usize,
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
    crate::FabricationSnapshot::capture(project)?.render(options)
}
impl crate::FabricationSnapshot {
    pub fn render(&self, options: FabricationOptions) -> io::Result<FabricationPackage> {
        render_fabrication(&self.project, options)
    }
    pub fn write(&self, options: FabricationOptions, path: &Path, native: Option<&Path>) -> io::Result<(PathBuf, FabricationManifest)> {
        publish_fabrication(self, options, path, native)
    }
}
// Count only emitted pad geometry; absence must preserve the old two-primitive
// copper + legacy-mask eligibility, and intentional omission adds no primitive.
fn emitted_pad_graphics(s: &PadApertureSummary) -> usize {
    s.explicit_pads.saturating_add(s.legacy_pads).saturating_add(s.legacy_pads).saturating_add(s.mask_rectangles).saturating_add(if s.include_paste {
        s.paste_rectangles
    } else {
        0
    })
}

fn render_fabrication(project: &Project, options: FabricationOptions) -> io::Result<FabricationPackage> {
    if !options.mask_expansion_mm.is_finite() || !(0.0..=10.0).contains(&options.mask_expansion_mm) {
        return Err(invalid("mask_expansion_mm must be finite and between 0 and 10 mm"));
    }
    viamind_doc::footprint_holes::preflight_project(project, None).map_err(invalid)?;
    let mut hole_summary = NonplatedHoleSummary {
        include_npth: options.include_npth,
        authored_components: 0,
        explicit_empty_components: 0,
        physical_drills: 0,
        tool_diameters: 0,
    };
    // Refuse authored intent without its independent output before allocating tools/graphics.
    for part in project.board.parts.values() {
        if let Some(holes) = &part.holes {
            if !options.include_npth {
                return Err(invalid("authored nonplated holes require include_npth=true, including explicit empty hole sets"));
            }
            hole_summary.authored_components += 1;
            hole_summary.explicit_empty_components += usize::from(holes.is_empty());
            hole_summary.physical_drills = hole_summary.physical_drills.saturating_add(holes.len());
            viamind_doc::footprint_holes::validate_part_holes(holes).map_err(invalid)?;
        }
    }
    let mut npth = BTreeMap::new();
    if options.include_npth {
        for part in project.board.parts.values() {
            for hole in part.holes.iter().flatten() {
                let resolved = viamind_doc::footprint_holes::resolve_hole(part, hole).map_err(invalid)?;
                let position = point(resolved.center)?;
                if !npth.contains_key(&resolved.diameter.0) && npth.len() == 99 {
                    return Err(invalid("XNC supports at most99 distinct nonplated drill diameters"));
                }
                npth.entry(resolved.diameter.0).or_insert_with(Vec::new).push((part.id, hole.id, position));
            }
        }
    }
    hole_summary.tool_diameters = npth.len();
    let outline = project.board.outline.as_ref().ok_or_else(|| invalid("fabrication export requires an explicit board outline"))?;
    let mut pads = 0usize;
    let mut aperture_summary = PadApertureSummary {
        include_paste: options.include_paste,
        explicit_pads: 0,
        legacy_pads: 0,
        mask_rectangles: 0,
        mask_omissions: 0,
        paste_rectangles: 0,
        paste_omissions: 0,
        top_mask_rectangles: 0,
        bottom_mask_rectangles: 0,
        top_paste_rectangles: 0,
        bottom_paste_rectangles: 0,
        explicit_coverage_complete: false,
    };
    let mut artwork = FootprintArtworkSummary {
        include_silkscreen: options.include_silkscreen,
        explicit_components: 0,
        top_silkscreen_segments: 0,
        bottom_silkscreen_segments: 0,
        courtyard_count: 0,
    };
    let mut artwork_primitives = 0usize;
    for part in project.board.parts.values() {
        if let Some(art) = &part.artwork {
            artwork_primitives = artwork_primitives.saturating_add(art.primitive_count());
            if artwork_primitives > MAX_OBJECTS {
                return Err(invalid("fabrication artwork exceeds200000 primitives"));
            }
            art.validate().map_err(invalid)?;
            artwork.explicit_components += 1;
            artwork.courtyard_count += usize::from(art.courtyard.is_some());
            if part.side == Side::Top {
                artwork.top_silkscreen_segments += art.silkscreen.len();
            } else {
                artwork.bottom_silkscreen_segments += art.silkscreen.len();
            }
        }
        let explicit = part.pads.as_ref().ok_or_else(|| {
            invalid(format!("component {} has no explicit pads; legacy graphical footprint previews cannot be fabricated", part.designator))
        })?;
        pads = pads.saturating_add(explicit.0.len());
        for pad in &explicit.0 {
            if let Some(a) = &pad.apertures {
                a.validate_at(pad.position).map_err(invalid)?;
                if !options.include_paste && a.paste.is_some() {
                    return Err(invalid("explicit paste geometry requires include_paste=true; refusing silent omission"));
                }
                aperture_summary.explicit_pads += 1;
                aperture_summary.mask_rectangles += usize::from(a.mask.is_some());
                aperture_summary.mask_omissions += usize::from(a.mask.is_none());
                aperture_summary.paste_rectangles += usize::from(a.paste.is_some());
                aperture_summary.paste_omissions += usize::from(a.paste.is_none());
                if part.side == Side::Top {
                    aperture_summary.top_mask_rectangles += usize::from(a.mask.is_some());
                    aperture_summary.top_paste_rectangles += usize::from(a.paste.is_some());
                } else {
                    aperture_summary.bottom_mask_rectangles += usize::from(a.mask.is_some());
                    aperture_summary.bottom_paste_rectangles += usize::from(a.paste.is_some());
                }
            } else {
                aperture_summary.legacy_pads += 1;
            }
        }
    }
    aperture_summary.explicit_coverage_complete = aperture_summary.legacy_pads == 0;
    let pad_graphics = emitted_pad_graphics(&aperture_summary);
    let objects = pad_graphics
        .saturating_add(project.board.tracks.len())
        .saturating_add(project.board.vias.len().saturating_mul(5))
        .saturating_add(outline.rings().map(|r| r.points.len()).sum::<usize>());
    let objects = objects.saturating_add(artwork_primitives).saturating_add(hole_summary.physical_drills);
    if objects > MAX_OBJECTS {
        return Err(invalid("fabrication export supports at most 200000 primitives"));
    }
    outline.validate().map_err(|e| invalid(e.to_string()))?;
    viamind_doc::file::validate_pads(project).map_err(|e| invalid(e.to_string()))?;
    viamind_doc::file::validate_copper(project).map_err(|e| invalid(e.to_string()))?;
    let expansion = (options.mask_expansion_mm * 1_000_000.0).round() as i64;
    let mut copper = [Vec::new(), Vec::new()];
    let mut mask = [Vec::new(), Vec::new()];
    let mut paste = [Vec::new(), Vec::new()];
    let mut legend = [Vec::new(), Vec::new()];
    for part in project.board.parts.values() {
        let side = usize::from(part.side == Side::Bottom);
        if options.include_silkscreen
            && let Some(art) = &part.artwork
        {
            for line in &art.silkscreen {
                let transform = |p: Point| {
                    let (x, y) = part.local_to_world_mm(p.x.to_mm(), p.y.to_mm());
                    rotated_center(x, y)
                };
                legend[side].push(Graphic {
                    aperture: circle(line.width.0, ""),
                    start: transform(line.start)?,
                    end: Some(transform(line.end)?),
                    rotation: 0,
                });
            }
        }
        for pad in &part.pads.as_ref().unwrap().0 {
            let (x, y) = part.local_to_world_mm(pad.position.x.to_mm(), pad.position.y.to_mm());
            let center = rotated_center(x, y)?;
            let aperture = Aperture { width: pad.width.0, height: Some(pad.height.0), function: "SMDPad,CuDef" };
            copper[side].push(flash(aperture, center, part.rotation_mdeg));
            for (kind, graphics) in [(ApertureKind::Mask, &mut mask[side]), (ApertureKind::Paste, &mut paste[side])] {
                let dimensions = match resolve_aperture(pad.apertures.as_ref(), kind) {
                    ApertureResolution::Legacy if kind == ApertureKind::Mask => Some((pad.width.0 + 2 * expansion, pad.height.0 + 2 * expansion)),
                    ApertureResolution::Rectangle(r) => Some((r.width.0, r.height.0)),
                    _ => None,
                };
                if let Some((width, height)) = dimensions {
                    graphics.push(flash(Aperture { width, height: Some(height), function: "" }, center, part.rotation_mdeg));
                }
            }
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
    if drills.len() > 99 {
        return Err(invalid("XNC supports at most 99 distinct through-via drill diameters"));
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
    if options.include_paste {
        for (name, function, graphics) in [("top-paste.gbr", "Paste,Top", &paste[0]), ("bottom-paste.gbr", "Paste,Bot", &paste[1])] {
            files.insert(name.into(), gerber(graphics, function, false, &id));
        }
    }
    if options.include_silkscreen {
        for (name, function, graphics) in [("top-silkscreen.gbr", "Legend,Top", &legend[0]), ("bottom-silkscreen.gbr", "Legend,Bot", &legend[1])] {
            files.insert(name.into(), gerber(graphics, function, false, &id));
        }
    }
    let mut drill = String::from("; Viamind plated through vias, finished diameters, aligned top view\nM48\nMETRIC\n");
    for (i, (diameter, _)) in drills.iter().enumerate() {
        writeln!(drill, "T{:02}C{}", i + 1, decimal(*diameter, 6)).unwrap();
    }
    drill.push_str("%\nG05\n");
    for (i, (_, positions)) in drills.iter().enumerate() {
        writeln!(drill, "T{:02}", i + 1).unwrap();
        for &(x, y) in positions {
            writeln!(drill, "X{}Y{}", decimal(x, 6), decimal(y, 6)).unwrap();
        }
    }
    drill.push_str("M30\n");
    files.insert("plated-through.xnc".into(), drill);
    if options.include_npth {
        let mut output = format!(
            "; Viamind circular nonplated through holes, aligned top view\n; #@! TF.FileFunction,NonPlated,1,2,NPTH\n; #@! TF.SameCoordinates,{id}\nM48\nMETRIC\n"
        );
        for (i, diameter) in npth.keys().enumerate() {
            writeln!(output, "T{:02}C{}", i + 1, decimal(*diameter, 6)).unwrap();
        }
        output.push_str("%\nG05\n");
        for (i, positions) in npth.values_mut().enumerate() {
            positions.sort_by_key(|(part, hole, _)| (*part, *hole));
            writeln!(output, "T{:02}", i + 1).unwrap();
            for &(_, _, (x, y)) in positions.iter() {
                writeln!(output, "X{}Y{}", decimal(x, 6), decimal(y, 6)).unwrap();
            }
        }
        output.push_str("M30\n");
        files.insert("nonplated-through.xnc".into(), output);
    }
    let mut manifest=FabricationManifest { nonplated_holes: options.include_npth.then_some(hole_summary), pad_apertures: (options.include_paste || aperture_summary.explicit_pads > 0).then_some(aperture_summary), footprint_artwork: (options.include_silkscreen || artwork.explicit_components > 0).then_some(artwork), project_id:id,units:"mm (Gerber 5.6, 1 nm; XNC explicit decimals)".into(),coordinate_system:"Model origin; X right; Y up (-model Y); top view for every layer; bottom files are not mirrored".into(),mask_expansion_nm:expansion,smd_pad_count:pads,track_count:project.board.tracks.len(),through_via_count:project.board.vias.len(),cutout_count:outline.cutouts.len(),files:files.keys().cloned().chain(std::iter::once("manifest.json".into())).collect(),exclusions:vec!["Scoped geometry export, not fabrication readiness or clearance/continuity certification".into(),"No legacy previews, inner copper, pours, arcs, component through holes/slots, blind/buried/microvias, paste, silkscreen, netlist/X3 assembly or material/finish/stackup instructions".into(),"Mask expansion is an export parameter; all vias untented; no manufacturer rules inferred".into()] };
    if options.include_npth {
        manifest.exclusions[1] = manifest.exclusions[1].replace("component through holes/slots", "plated component through holes/slots");
        manifest.exclusions.push("Circular nonplated holes use explicit component geometry on both sides; absent hole policy is uninspected intent. No slots, drill suitability, material or manufacturer verification.".into());
    }
    if options.include_paste {
        manifest.exclusions[1] = manifest.exclusions[1].replace("paste, ", "");
        manifest.exclusions.push("Paste uses only explicitly stored pad rectangles; legacy pads have no inferred paste. All vias remain untented with no paste. No stencil suitability checks.".into());
    }
    if options.include_silkscreen {
        manifest.exclusions[1] = manifest.exclusions[1].replace("silkscreen, ", "");
    }
    if let Some(a) = &manifest.pad_apertures {
        manifest.exclusions[2] =
            "Mask expansion applies only to legacy pads and untented vias; explicit mask rectangles are absolute. No manufacturer rules inferred."
                .into();
        if !a.explicit_coverage_complete {
            manifest
                .exclusions
                .push("Incomplete explicit aperture coverage: legacy SMD pads have no paste policy; absent data is not intentional omission.".into());
        }
    }
    if manifest.footprint_artwork.is_some() {
        manifest.exclusions.push(if options.include_silkscreen {
            "Stored silkscreen straight strokes only; no synthetic bodies, text, clipping, silk-to-mask or courtyard clearance checks. Courtyards are excluded.".into()
        } else { "Explicit footprint artwork is excluded; no silkscreen or courtyard checks.".into() });
    }
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
    crate::FabricationSnapshot::capture(project)?.write(options, path, native)
}
fn publish_fabrication(
    snapshot: &crate::FabricationSnapshot,
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
    let package = snapshot.render(options)?;
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

#[cfg(test)]
mod aperture_admission_tests {
    use super::*;
    #[test]
    fn actual_pad_emissions_preserve_legacy_limit_and_do_not_charge_omissions() {
        let mut s = PadApertureSummary {
            include_paste: false,
            explicit_pads: 0,
            legacy_pads: 99_998,
            mask_rectangles: 0,
            mask_omissions: 0,
            paste_rectangles: 0,
            paste_omissions: 0,
            top_mask_rectangles: 0,
            bottom_mask_rectangles: 0,
            top_paste_rectangles: 0,
            bottom_paste_rectangles: 0,
            explicit_coverage_complete: false,
        };
        // Four outline edges leave exactly the original legacy admission bound.
        assert_eq!(emitted_pad_graphics(&s) + 4, MAX_OBJECTS);
        s.legacy_pads += 1;
        assert!(emitted_pad_graphics(&s) + 4 > MAX_OBJECTS);
        s.legacy_pads = 0;
        s.explicit_pads = 199_996;
        s.mask_omissions = s.explicit_pads;
        s.paste_omissions = s.explicit_pads;
        assert_eq!(emitted_pad_graphics(&s) + 4, MAX_OBJECTS);
        s.include_paste = true;
        assert_eq!(emitted_pad_graphics(&s) + 4, MAX_OBJECTS);
        s.mask_rectangles = 1;
        assert_eq!(emitted_pad_graphics(&s) + 4, MAX_OBJECTS + 1);
        s.mask_rectangles = 0;
        s.paste_rectangles = 1;
        assert_eq!(emitted_pad_graphics(&s) + 4, MAX_OBJECTS + 1);
        s.include_paste = false;
        assert_eq!(emitted_pad_graphics(&s) + 4, MAX_OBJECTS);
    }
}
