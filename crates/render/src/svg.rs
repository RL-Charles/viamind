//! Deterministic, bounded, self-contained board inspection views.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    io::{self, Write},
    path::{Path, PathBuf},
};
use viamind_doc::{CopperLayer, Project, Side};

const MAX_COORD: i64 = 100_000_000_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const OUTLINE_HALF_STROKE: i64 = 50_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewLayer {
    BottomCopper,
    TopCopper,
    Outline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewBounds {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}
impl ViewBounds {
    fn validate(self) -> io::Result<Self> {
        if [self.min_x, self.min_y, self.max_x, self.max_y].iter().any(|v| v.unsigned_abs() > MAX_COORD as u64)
            || self.min_x >= self.max_x
            || self.min_y >= self.max_y
        {
            return Err(invalid("region_nm requires positive extents and coordinates within ±100000 mm"));
        }
        Ok(self)
    }
    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
    fn expand(self, n: i64) -> Self {
        Self { min_x: self.min_x - n, min_y: self.min_y - n, max_x: self.max_x + n, max_y: self.max_y + n }
    }
}
fn default_layers() -> Vec<ViewLayer> {
    vec![ViewLayer::BottomCopper, ViewLayer::TopCopper, ViewLayer::Outline]
}
fn default_padding() -> i64 {
    1_000_000
}
fn default_width() -> u32 {
    1600
}
fn default_height() -> u32 {
    1000
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SvgOptions {
    #[serde(default = "default_layers")]
    pub layers: Vec<ViewLayer>,
    /// Explicit model-space nanometre region; otherwise fit selected supported geometry.
    #[serde(default)]
    pub region_nm: Option<ViewBounds>,
    /// Automatic fit padding only, between zero and 1000 mm.
    #[serde(default = "default_padding")]
    pub padding_nm: i64,
    #[serde(default = "default_width")]
    pub width_px: u32,
    #[serde(default = "default_height")]
    pub height_px: u32,
}
impl Default for SvgOptions {
    fn default() -> Self {
        Self { layers: default_layers(), region_nm: None, padding_nm: default_padding(), width_px: default_width(), height_px: default_height() }
    }
}
impl SvgOptions {
    pub fn validate(&self) -> io::Result<()> {
        if self.layers.is_empty() || self.layers.len() > 32 {
            return Err(invalid("layers requires 1–32 entries from top_copper, bottom_copper, outline"));
        }
        if !(1..=4096).contains(&self.width_px) || !(1..=4096).contains(&self.height_px) {
            return Err(invalid("view pixel dimensions must be between 1 and 4096"));
        }
        if !(0..=1_000_000_000).contains(&self.padding_nm) {
            return Err(invalid("padding_nm must be between zero and 1000 mm"));
        }
        if let Some(region) = self.region_nm {
            region.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SvgMetadata {
    pub region_nm: ViewBounds,
    pub width_px: u32,
    pub height_px: u32,
    pub layers: Vec<ViewLayer>,
    pub units: &'static str,
    pub axes: &'static str,
    pub pad_count: usize,
    pub track_count: usize,
    pub through_via_count: usize,
    pub omitted_legacy_parts: usize,
    pub replaced_invalid_xml_text: bool,
    pub exclusions: Vec<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct BoardSvg {
    pub svg: String,
    pub metadata: SvgMetadata,
}
fn invalid(s: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s.into())
}
// Values are in tenths of a nanometre, retaining exact half-nm radii/rectangle edges.
fn mm10(n: i128) -> String {
    let a = n.abs();
    format!("{}{}.{:07}", if n < 0 { "-" } else { "" }, a / 10_000_000, a % 10_000_000)
}
fn mm(n: i64) -> String {
    mm10(i128::from(n) * 10)
}
fn half(n: i64) -> String {
    mm10(i128::from(n) * 5)
}
fn xml(text: &str, replaced: &mut bool) -> io::Result<String> {
    if text.len() > 4096 {
        return Err(invalid("SVG metadata strings must not exceed 4096 UTF-8 bytes"));
    }
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\r' => out.push_str("&#13;"),
            '\t' | '\n' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}' => out.push(c),
            _ => {
                out.push('\u{fffd}');
                *replaced = true;
            }
        }
    }
    Ok(out)
}
struct Shape {
    layer: ViewLayer,
    bounds: ViewBounds,
    xml: String,
}
fn center(x: f64, y: f64) -> io::Result<(i64, i64)> {
    if !x.is_finite() || !y.is_finite() || x.abs() > 100_000.0 || y.abs() > 100_000.0 {
        return Err(invalid("rotated pad exceeds SVG coordinate range"));
    }
    Ok(((x * 1_000_000.0).round() as i64, (y * 1_000_000.0).round() as i64))
}
fn extent(x: i64, y: i64, r: i64) -> ViewBounds {
    ViewBounds { min_x: x - r, min_y: y - r, max_x: x + r, max_y: y + r }
}
fn rect_attrs(r: ViewBounds, origin: (i64, i64)) -> String {
    format!(
        "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
        mm(r.min_x - origin.0),
        mm(r.min_y - origin.1),
        mm(r.max_x - r.min_x),
        mm(r.max_y - r.min_y)
    )
}

/// Maximum total UTF-8 text copied into a board view snapshot.
pub const MAX_BOARD_VIEW_TEXT: usize = 2 * 1024 * 1024;
fn validate_board_budget(project: &Project) -> io::Result<()> {
    let mut count = project.board.parts.len().saturating_add(project.board.tracks.len()).saturating_add(project.board.vias.len().saturating_mul(3));
    if count > 100_000 {
        return Err(invalid("SVG view supports at most 100000 primitives"));
    }
    for part in project.board.parts.values() {
        count = count.saturating_add(part.pads.as_ref().map_or(0, |p| p.0.len()));
        if count > 100_000 {
            return Err(invalid("SVG view supports at most 100000 primitives"));
        }
    }
    if let Some(outline) = &project.board.outline {
        for ring in outline.rings() {
            count = count.saturating_add(1).saturating_add(ring.points.len());
            if count > 100_000 {
                return Err(invalid("SVG view supports at most 100000 primitives"));
            }
        }
    }
    let mut text = 0usize;
    let mut add = |s: &str| -> io::Result<()> {
        if s.len() > 4096 {
            return Err(invalid("SVG metadata strings must not exceed 4096 UTF-8 bytes"));
        }
        text = text.saturating_add(s.len());
        if text > MAX_BOARD_VIEW_TEXT {
            return Err(invalid("board view metadata exceeds 2 MiB aggregate UTF-8 text"));
        }
        Ok(())
    };
    add(&project.name)?;
    for part in project.board.parts.values() {
        add(&part.designator)?;
        add(&part.value)?;
        add(&part.footprint)?;
        if let Some(pads) = &part.pads {
            for pad in &pads.0 {
                add(&pad.number)?;
                if let Some(net) = &pad.net {
                    add(net)?;
                }
            }
        }
    }
    for track in project.board.tracks.values() {
        if let Some(net) = &track.net {
            add(net)?;
        }
    }
    for via in project.board.vias.values() {
        if let Some(net) = &via.net {
            add(net)?;
        }
    }
    Ok(())
}
/// Bounded board-only copy. Unrelated schematic content is never cloned or validated.
pub fn board_view_snapshot(project: &Project) -> io::Result<Project> {
    validate_board_budget(project)?;
    Ok(Project { id: project.id, name: project.name.clone(), board: project.board.clone(), schematic: Default::default() })
}

/// Render model geometry without inferred legacy footprint previews or model changes.
pub fn board_svg(project: &Project, options: &SvgOptions) -> io::Result<BoardSvg> {
    board_svg_impl(project, options, None)
}

// Normalize at the shared I/O boundary before the rasterizer stores f32 paths.
pub(crate) fn board_svg_for_pixels(project: &Project, options: &SvgOptions) -> io::Result<BoardSvg> {
    let region = match options.region_nm {
        Some(region) => region,
        None => board_svg(project, options)?.metadata.region_nm,
    };
    board_svg_impl(project, options, Some((region.min_x, region.min_y)))
}

fn board_svg_impl(project: &Project, options: &SvgOptions, raster_origin: Option<(i64, i64)>) -> io::Result<BoardSvg> {
    options.validate()?;
    let origin = raster_origin.unwrap_or((0, 0));
    let x = |n: i64| mm(n - origin.0);
    let y = |n: i64| mm(n - origin.1);
    validate_board_budget(project)?;
    viamind_doc::file::validate_pads(project).map_err(|e| invalid(e.to_string()))?;
    viamind_doc::file::validate_copper(project).map_err(|e| invalid(e.to_string()))?;
    if let Some(outline) = &project.board.outline {
        outline.validate().map_err(|e| invalid(e.to_string()))?;
    }
    let layers: BTreeSet<_> = options.layers.iter().copied().collect();
    let mut replaced = false;
    let name = xml(&project.name, &mut replaced)?;
    let mut shapes = Vec::new();
    let mut pads = 0;
    let mut tracks = 0;
    let mut omitted = 0;
    let mut size = 0;
    for part in project.board.parts.values() {
        let Some(explicit) = &part.pads else {
            omitted += 1;
            continue;
        };
        let layer = if part.side == Side::Bottom { ViewLayer::BottomCopper } else { ViewLayer::TopCopper };
        if !layers.contains(&layer) {
            continue;
        }
        for pad in &explicit.0 {
            let (wx, wy) = part.local_to_world_mm(pad.position.x.to_mm(), pad.position.y.to_mm());
            let (cx, cy) = center(wx, wy)?;
            let angle = -(f64::from(part.rotation_mdeg) / 1000.0);
            let (s, c) = angle.to_radians().sin_cos();
            let rx = (pad.width.0 as f64 * c.abs() + pad.height.0 as f64 * s.abs()) / 2.0;
            let ry = (pad.width.0 as f64 * s.abs() + pad.height.0 as f64 * c.abs()) / 2.0;
            let bounds = ViewBounds {
                min_x: (cx as f64 - rx).floor() as i64,
                min_y: (cy as f64 - ry).floor() as i64,
                max_x: (cx as f64 + rx).ceil() as i64,
                max_y: (cy as f64 + ry).ceil() as i64,
            };
            let title = xml(
                &format!("{} pad {} · {} · {}", part.designator, pad.number, part.value, pad.net.as_deref().unwrap_or("unassigned")),
                &mut replaced,
            )?;
            let shape = format!(
                "<g id=\"pad-{}\" transform=\"translate({} {}) rotate({angle:.3})\"><title>{title}</title><rect x=\"-{}\" y=\"-{}\" width=\"{}\" height=\"{}\"/></g>\n",
                pad.id,
                x(cx),
                y(cy),
                half(pad.width.0),
                half(pad.height.0),
                mm(pad.width.0),
                mm(pad.height.0)
            );
            size += shape.len();
            shapes.push(Shape { layer, bounds, xml: shape });
            pads += 1;
            if size > MAX_BYTES {
                return Err(invalid("SVG view exceeds 16 MiB"));
            }
        }
    }
    for t in project.board.tracks.values() {
        let layer = if t.layer == CopperLayer::Bottom { ViewLayer::BottomCopper } else { ViewLayer::TopCopper };
        if !layers.contains(&layer) {
            continue;
        }
        let bounds = extent(t.start.x.0, t.start.y.0, (t.width.0 + 1) / 2).union(extent(t.end.x.0, t.end.y.0, (t.width.0 + 1) / 2));
        let title = xml(&format!("Track {} · {}", t.id, t.net.as_deref().unwrap_or("unassigned")), &mut replaced)?;
        let shape = format!(
            "<path id=\"track-{}\" d=\"M{} {} L{} {}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"{}\" stroke-linecap=\"round\"><title>{title}</title></path>\n",
            t.id,
            x(t.start.x.0),
            y(t.start.y.0),
            x(t.end.x.0),
            y(t.end.y.0),
            mm(t.width.0)
        );
        size += shape.len();
        shapes.push(Shape { layer, bounds, xml: shape });
        tracks += 1;
        if size > MAX_BYTES {
            return Err(invalid("SVG view exceeds 16 MiB"));
        }
    }
    for v in project.board.vias.values() {
        for layer in [ViewLayer::BottomCopper, ViewLayer::TopCopper] {
            if !layers.contains(&layer) {
                continue;
            }
            let title = xml(
                &format!("Through via {} · {} · finished drill {} mm", v.id, v.net.as_deref().unwrap_or("unassigned"), mm(v.drill.0)),
                &mut replaced,
            )?;
            let shape = format!(
                "<circle id=\"via-{layer:?}-{}\" cx=\"{}\" cy=\"{}\" r=\"{}\"><title>{title}</title></circle>\n",
                v.id,
                x(v.position.x.0),
                y(v.position.y.0),
                half(v.diameter.0)
            );
            size += shape.len();
            shapes.push(Shape { layer, bounds: extent(v.position.x.0, v.position.y.0, (v.diameter.0 + 1) / 2), xml: shape });
            if size > MAX_BYTES {
                return Err(invalid("SVG view exceeds 16 MiB"));
            }
        }
    }
    if layers.contains(&ViewLayer::Outline)
        && let Some(outline) = &project.board.outline
    {
        let mut d = String::new();
        let mut bounds = None;
        for ring in outline.rings() {
            for (i, p) in ring.points.iter().enumerate() {
                write!(d, "{}{} {} ", if i == 0 { "M" } else { "L" }, x(p.x.0), y(p.y.0)).unwrap();
                let b = extent(p.x.0, p.y.0, OUTLINE_HALF_STROKE);
                bounds = Some(bounds.map_or(b, |old: ViewBounds| old.union(b)));
            }
            d.push_str("Z ");
        }
        shapes.push(Shape {
                layer: ViewLayer::Outline,
                bounds: bounds.unwrap(),
                xml: format!(
                    "<path id=\"board-outline\" d=\"{d}\" fill=\"#25302b\" fill-rule=\"evenodd\" stroke=\"#d8ddd9\" stroke-width=\"0.1\" stroke-linejoin=\"round\"/>\n"
                ),
            });
    }
    for s in &shapes {
        s.bounds.validate()?;
    }
    let region = match options.region_nm {
        Some(r) => r,
        None => shapes
            .iter()
            .map(|s| s.bounds)
            .reduce(ViewBounds::union)
            .map_or(ViewBounds { min_x: -5_000_000, min_y: -5_000_000, max_x: 5_000_000, max_y: 5_000_000 }, |r| r.expand(options.padding_nm)),
    }
    .validate()?;
    if raster_origin.is_some() {
        let scale = (f64::from(options.width_px) / (region.max_x - region.min_x) as f64)
            .min(f64::from(options.height_px) / (region.max_y - region.min_y) as f64);
        let holes = project.board.vias.values().map(|v| extent(v.position.x.0, v.position.y.0, (v.drill.0 + 1) / 2));
        for b in shapes.iter().map(|s| s.bounds).chain(holes) {
            if b.max_x < region.min_x || b.min_x > region.max_x || b.max_y < region.min_y || b.min_y > region.max_y {
                continue;
            }
            let magnitude =
                [b.min_x - origin.0, b.max_x - origin.0, b.min_y - origin.1, b.max_y - origin.1].into_iter().map(i64::unsigned_abs).max().unwrap()
                    as f64;
            if magnitude * f64::from(f32::EPSILON) * scale > 0.125 {
                return Err(invalid(
                    "PNG region is too small relative to intersecting geometry for the raster precision budget; zoom out or hide the large layer",
                ));
            }
        }
    }
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"xMidYMid meet\" role=\"img\" aria-labelledby=\"view-title view-description\">\n<title id=\"view-title\">{name}</title>\n<desc id=\"view-description\">PCB inspection view; model origin, mm, X right/Y down; legacy previews omitted; not fabrication certification.</desc>\n<defs><clipPath id=\"view-clip\" clipPathUnits=\"userSpaceOnUse\"><rect {}/></clipPath>\n<mask id=\"through-holes\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" {} style=\"mask-type:luminance\"><rect {} fill=\"white\"/>\n",
        options.width_px,
        options.height_px,
        x(region.min_x),
        y(region.min_y),
        mm(region.max_x - region.min_x),
        mm(region.max_y - region.min_y),
        rect_attrs(region, origin),
        rect_attrs(region, origin),
        rect_attrs(region, origin)
    );
    for v in project.board.vias.values() {
        writeln!(svg, "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"black\"/>", x(v.position.x.0), y(v.position.y.0), half(v.drill.0)).unwrap();
    }
    svg.push_str("</mask></defs>\n<g clip-path=\"url(#view-clip)\"><g mask=\"url(#through-holes)\">\n");
    // Substrate first; top copper last. Request order does not affect stacking.
    for (layer, color) in [(ViewLayer::Outline, "#d8ddd9"), (ViewLayer::BottomCopper, "#4773dc"), (ViewLayer::TopCopper, "#d65348")] {
        if !layers.contains(&layer) {
            continue;
        }
        writeln!(svg, "<g id=\"layer-{layer:?}\" fill=\"{color}\" color=\"{color}\">").unwrap();
        for shape in shapes.iter().filter(|s| s.layer == layer) {
            svg.push_str(&shape.xml);
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</g></g></svg>\n");
    if svg.len() > MAX_BYTES {
        return Err(invalid("SVG view exceeds 16 MiB"));
    }
    Ok(BoardSvg {
        svg,
        metadata: SvgMetadata {
            region_nm: region,
            width_px: options.width_px,
            height_px: options.height_px,
            layers: layers.into_iter().collect(),
            units: "mm; model coordinates integer nm",
            axes: "Model origin; X right; Y down; top view; bottom layers not globally mirrored",
            pad_count: pads,
            track_count: tracks,
            through_via_count: project.board.vias.len(),
            omitted_legacy_parts: omitted,
            replaced_invalid_xml_text: replaced,
            exclusions: vec![
                "Legacy footprint previews, schematic views, inner copper and pours are not rendered",
                "Inspection view; no fabrication or electrical certification",
            ],
        },
    })
}

/// Atomically publish a NEW SVG file; protect the active native project path.
pub fn write_svg(svg: &str, path: &Path, native: Option<&Path>) -> io::Result<PathBuf> {
    write_view_bytes(svg.as_bytes(), path, native, MAX_BYTES, "SVG", ".viamind-svg-")
}

pub(crate) fn write_view_bytes(bytes: &[u8], path: &Path, native: Option<&Path>, limit: usize, kind: &str, prefix: &str) -> io::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(invalid(format!("{kind} output path must not be empty")));
    }
    if bytes.len() > limit {
        return Err(invalid(format!("{kind} output exceeds its byte limit")));
    }
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or_else(|| invalid(format!("{kind} output requires a parent directory")))?;
    let canonical = parent.canonicalize()?.join(path.file_name().ok_or_else(|| invalid(format!("{kind} output requires a filename")))?);
    if let Some(native) = native {
        let native = native.canonicalize().unwrap_or(std::path::absolute(native)?);
        if canonical == native || path.canonicalize().is_ok_and(|p| p == native) {
            return Err(invalid(format!("{kind} output must differ from active native project path")));
        }
    }
    let mut temporary = tempfile::Builder::new().prefix(prefix).tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&path).map_err(|e| e.error)?;
    Ok(path)
}
