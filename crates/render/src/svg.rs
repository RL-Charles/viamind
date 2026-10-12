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
    BottomMask,
    TopMask,
    BottomPaste,
    TopPaste,
    Outline,
    BottomSilkscreen,
    TopSilkscreen,
    BottomCourtyard,
    TopCourtyard,
    NonplatedHoles,
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
            return Err(invalid("layers requires 1–32 entries from copper, outline, silkscreen or courtyard layers"));
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonplated_holes: Option<NonplatedHoleViewMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad_apertures: Option<ApertureViewMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkViewMetadata>,
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
#[derive(Clone, Debug, Serialize)]
pub struct NonplatedHoleViewMetadata {
    pub holes: Vec<NonplatedHoleView>,
    pub rendered_holes: usize,
    pub checks_not_performed: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct NonplatedHoleView {
    pub part_id: viamind_doc::ObjectId,
    pub hole_id: viamind_doc::ObjectId,
    pub key: String,
    pub role: &'static str,
    pub center_nm: viamind_geom::Point,
    pub diameter_nm: viamind_geom::Nm,
}
#[derive(Clone, Debug, Serialize)]
pub struct ArtworkViewMetadata {
    pub silkscreen_segments: usize,
    pub courtyards: usize,
    pub rendered_segments: usize,
    pub checks_not_performed: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct ApertureViewMetadata {
    pub pads: Vec<PadOpeningView>,
    pub legacy_mask_policy: &'static str,
    pub checks_not_performed: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct PadOpeningView {
    pub pad_id: viamind_doc::ObjectId,
    pub mask: &'static str,
    pub paste: &'static str,
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
    viamind_doc::footprint_holes::validate_physical_identities(project).map_err(invalid)?;
    let policy = viamind_doc::named_net_rule_budget(&project.board.net_rules).map_err(invalid)?;
    let mut count = project.board.parts.len().saturating_add(project.board.tracks.len()).saturating_add(project.board.vias.len().saturating_mul(3));
    count = count.saturating_add(policy.rows);
    if count > 100_000 {
        return Err(invalid("SVG view supports at most 100000 primitives"));
    }
    for part in project.board.parts.values() {
        if let Some(holes) = &part.holes {
            count = count.saturating_add(holes.len());
            if count > 100_000 {
                return Err(invalid("SVG view supports at most100000 primitives including mechanical drills"));
            }
            viamind_doc::footprint_holes::validate_part_holes(holes).map_err(invalid)?;
            for hole in holes {
                viamind_doc::footprint_holes::resolve_hole(part, hole).map_err(invalid)?;
            }
        }
        if let Some(artwork) = &part.artwork {
            artwork.validate().map_err(invalid)?;
            count = count.saturating_add(artwork.primitive_count());
        }
        count = count.saturating_add(part.pads.as_ref().map_or(0, |p| p.0.len()));
        if let Some(pads) = &part.pads {
            for p in &pads.0 {
                if let Some(a) = &p.apertures {
                    count = count.saturating_add(a.structural_rows());
                    a.validate_at(p.position).map_err(invalid)?;
                }
            }
        }
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
    let mut text = policy.text_bytes;
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
        if let Some(holes) = &part.holes {
            for hole in holes {
                add(&hole.key)?;
            }
        }
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
    Ok(Project { id: project.id, name: project.name.clone(), board: project.board.clone_without_assembly(), schematic: Default::default() })
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
    // Direct callers receive the same schematic exclusion as engine snapshots.
    // The normal board-only path borrows without another model copy.
    let shell;
    let project = if project.schematic.sheets.is_empty() && project.board.parts.values().all(|p| p.assembly.is_none()) {
        project
    } else {
        shell = board_view_snapshot(project)?;
        &shell
    };
    let origin = raster_origin.unwrap_or((0, 0));
    let x = |n: i64| mm(n - origin.0);
    let y = |n: i64| mm(n - origin.1);
    validate_board_budget(project)?;
    viamind_doc::file::validate_pads(project).map_err(|e| invalid(e.to_string()))?;
    viamind_doc::file::validate_copper(project).map_err(|e| invalid(e.to_string()))?;
    if let Some(outline) = &project.board.outline {
        outline.validate().map_err(|e| invalid(e.to_string()))?;
    }
    let mut layers: BTreeSet<_> = options.layers.iter().copied().collect();
    let authored_holes = project.board.parts.values().any(|p| p.holes.is_some());
    // Extend the unchanged legacy default only when the physical feature is authored.
    if authored_holes && layers == default_layers().into_iter().collect() {
        layers.insert(ViewLayer::NonplatedHoles);
    }
    let aperture_requested =
        layers.iter().any(|l| matches!(l, ViewLayer::TopMask | ViewLayer::BottomMask | ViewLayer::TopPaste | ViewLayer::BottomPaste));
    let mut aperture_rows = Vec::new();
    let via_shapes = 1 + [ViewLayer::TopCopper, ViewLayer::BottomCopper, ViewLayer::TopMask, ViewLayer::BottomMask]
        .into_iter()
        .filter(|l| layers.contains(l))
        .count();
    let mut emitted = project.board.tracks.len().saturating_add(project.board.vias.len().saturating_mul(via_shapes));
    emitted = emitted.saturating_add(project.board.parts.values().filter_map(|p| p.artwork.as_ref()).map(|a| a.primitive_count()).sum::<usize>());
    if layers.contains(&ViewLayer::Outline) {
        emitted = emitted.saturating_add(project.board.outline.as_ref().map_or(0, |o| o.rings().map(|r| r.points.len()).sum::<usize>()));
    }
    for part in project.board.parts.values() {
        if layers.contains(&ViewLayer::NonplatedHoles) {
            emitted = emitted.saturating_add(part.holes.as_ref().map_or(0, Vec::len));
        }
        let (cu, mask, paste) = if part.side == Side::Top {
            (ViewLayer::TopCopper, ViewLayer::TopMask, ViewLayer::TopPaste)
        } else {
            (ViewLayer::BottomCopper, ViewLayer::BottomMask, ViewLayer::BottomPaste)
        };
        if let Some(pads) = &part.pads {
            for p in &pads.0 {
                emitted = emitted
                    .saturating_add(usize::from(layers.contains(&cu)))
                    .saturating_add(usize::from(
                        layers.contains(&mask)
                            && !matches!(
                                viamind_doc::resolve_aperture(p.apertures.as_ref(), viamind_doc::ApertureKind::Mask),
                                viamind_doc::ApertureResolution::IntentionalOmit
                            ),
                    ))
                    .saturating_add(usize::from(
                        layers.contains(&paste)
                            && matches!(
                                viamind_doc::resolve_aperture(p.apertures.as_ref(), viamind_doc::ApertureKind::Paste),
                                viamind_doc::ApertureResolution::Rectangle(_)
                            ),
                    ));
            }
        }
    }
    if emitted > 100_000 {
        return Err(invalid("selected board view exceeds100000 emitted primitives before allocation"));
    }

    let mut replaced = false;
    let name = xml(&project.name, &mut replaced)?;
    let mut shapes = Vec::new();
    let mut pads = 0;
    let mut tracks = 0;
    let mut omitted = 0;
    let mut size = 0;
    let mut artwork_metadata = None;
    let mut hole_metadata = authored_holes.then(|| NonplatedHoleViewMetadata {
        holes: Vec::new(), rendered_holes: 0,
        checks_not_performed: "Circular physical drill geometry preview; no boolean copper clipping, manufacturer or fabrication suitability verification",
    });
    for part in project.board.parts.values() {
        let Some(holes) = &part.holes else { continue };
        for hole in holes {
            let resolved = viamind_doc::footprint_holes::resolve_hole(part, hole).map_err(invalid)?;
            let metadata = hole_metadata.as_mut().expect("authored feature");
            metadata.holes.push(NonplatedHoleView {
                part_id: part.id,
                hole_id: hole.id,
                key: hole.key.clone(),
                role: "nonplated_through_hole",
                center_nm: resolved.center,
                diameter_nm: resolved.diameter,
            });
            if !layers.contains(&ViewLayer::NonplatedHoles) {
                continue;
            }
            let bounds = extent(resolved.center.x.0, resolved.center.y.0, (resolved.diameter.0 + 1) / 2);
            bounds.validate()?;
            let key = xml(&hole.key, &mut replaced)?;
            let shape = format!(
                "<circle id=\"npth-{}\" data-role=\"nonplated_through_hole\" data-part=\"{}\" data-key=\"{key}\" cx=\"{}\" cy=\"{}\" r=\"{}\"><title>Nonplated through hole {key} · part {}</title></circle>\n",
                hole.id,
                part.id,
                x(resolved.center.x.0),
                y(resolved.center.y.0),
                half(resolved.diameter.0),
                part.id,
            );
            size += shape.len();
            if size > MAX_BYTES {
                return Err(invalid("SVG view exceeds16MiB"));
            }
            shapes.push(Shape { layer: ViewLayer::NonplatedHoles, bounds, xml: shape });
            metadata.rendered_holes += 1;
        }
    }
    for part in project.board.parts.values() {
        let Some(artwork) = &part.artwork else { continue };
        let metadata = artwork_metadata.get_or_insert(ArtworkViewMetadata {
            silkscreen_segments: 0,
            courtyards: 0,
            rendered_segments: 0,
            checks_not_performed: "Silkscreen clipping/collisions and courtyard clearance are not checked",
        });
        metadata.silkscreen_segments += artwork.silkscreen.len();
        metadata.courtyards += usize::from(artwork.courtyard.is_some());
        let (silk, court) = if part.side == Side::Bottom {
            (ViewLayer::BottomSilkscreen, ViewLayer::BottomCourtyard)
        } else {
            (ViewLayer::TopSilkscreen, ViewLayer::TopCourtyard)
        };
        let mut line = |a: viamind_geom::Point, b: viamind_geom::Point, width: i64, layer: ViewLayer, index: usize| -> io::Result<()> {
            if !layers.contains(&layer) {
                return Ok(());
            }
            let a = part.local_to_world_mm(a.x.to_mm(), a.y.to_mm());
            let b = part.local_to_world_mm(b.x.to_mm(), b.y.to_mm());
            let (ax, ay) = center(a.0, a.1)?;
            let (bx, by) = center(b.0, b.1)?;
            let r = width / 2 + width % 2;
            let bounds = ViewBounds { min_x: ax.min(bx) - r, min_y: ay.min(by) - r, max_x: ax.max(bx) + r, max_y: ay.max(by) + r };
            let xml = format!(
                "<line id=\"art-{}-{layer:?}-{index}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"currentColor\" stroke-width=\"{}\" stroke-linecap=\"round\"/>\n",
                part.id,
                x(ax),
                y(ay),
                x(bx),
                y(by),
                mm(width)
            );
            size += xml.len();
            if size > MAX_BYTES {
                return Err(invalid("SVG view exceeds16MiB"));
            }
            shapes.push(Shape { layer, bounds, xml });
            metadata.rendered_segments += 1;
            Ok(())
        };
        for (i, s) in artwork.silkscreen.iter().enumerate() {
            line(s.start, s.end, s.width.0, silk, i)?;
        }
        if let Some(c) = &artwork.courtyard {
            let corners = c.corners();
            for i in 0..4 {
                line(corners[i], corners[(i + 1) % 4], c.width.0, court, i)?;
            }
        }
    }
    for part in project.board.parts.values() {
        let Some(explicit) = &part.pads else {
            omitted += 1;
            continue;
        };
        let (copper, mask, paste) = if part.side == Side::Bottom {
            (ViewLayer::BottomCopper, ViewLayer::BottomMask, ViewLayer::BottomPaste)
        } else {
            (ViewLayer::TopCopper, ViewLayer::TopMask, ViewLayer::TopPaste)
        };
        for pad in &explicit.0 {
            if aperture_requested && (layers.contains(&mask) || layers.contains(&paste)) {
                let state = |kind| match viamind_doc::resolve_aperture(pad.apertures.as_ref(), kind) {
                    viamind_doc::ApertureResolution::Legacy => "legacy",
                    viamind_doc::ApertureResolution::IntentionalOmit => "intentional_omit",
                    viamind_doc::ApertureResolution::Rectangle(_) => "explicit_rectangle",
                };
                aperture_rows.push(PadOpeningView {
                    pad_id: pad.id,
                    mask: state(viamind_doc::ApertureKind::Mask),
                    paste: state(viamind_doc::ApertureKind::Paste),
                });
            }
            for (layer, width, height, role) in [
                (copper, Some(pad.width.0), Some(pad.height.0), "copper"),
                {
                    let r = viamind_doc::resolve_aperture(pad.apertures.as_ref(), viamind_doc::ApertureKind::Mask);
                    let d = match r {
                        viamind_doc::ApertureResolution::Legacy => Some((pad.width.0, pad.height.0)),
                        viamind_doc::ApertureResolution::Rectangle(r) => Some((r.width.0, r.height.0)),
                        _ => None,
                    };
                    (mask, d.map(|r| r.0), d.map(|r| r.1), "mask")
                },
                {
                    let r = viamind_doc::resolve_aperture(pad.apertures.as_ref(), viamind_doc::ApertureKind::Paste);
                    let d = match r {
                        viamind_doc::ApertureResolution::Rectangle(r) => Some((r.width.0, r.height.0)),
                        _ => None,
                    };
                    (paste, d.map(|r| r.0), d.map(|r| r.1), "paste")
                },
            ] {
                if !layers.contains(&layer) {
                    continue;
                }
                let (Some(width), Some(height)) = (width, height) else {
                    continue;
                };
                let (wx, wy) = part.local_to_world_mm(pad.position.x.to_mm(), pad.position.y.to_mm());
                let (cx, cy) = center(wx, wy)?;
                let angle = -(f64::from(part.rotation_mdeg) / 1000.0);
                let (s, c) = angle.to_radians().sin_cos();
                let rx = (width as f64 * c.abs() + height as f64 * s.abs()) / 2.0;
                let ry = (width as f64 * s.abs() + height as f64 * c.abs()) / 2.0;
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
                let identity = if role == "copper" { format!("pad-{}", pad.id) } else { format!("pad-{}-{role}", pad.id) };
                let shape = format!(
                    "<g id=\"{}\" transform=\"translate({} {}) rotate({angle:.3})\"><title>{title}</title><rect x=\"-{}\" y=\"-{}\" width=\"{}\" height=\"{}\"/></g>\n",
                    identity,
                    x(cx),
                    y(cy),
                    half(width),
                    half(height),
                    mm(width),
                    mm(height)
                );
                size += shape.len();
                shapes.push(Shape { layer, bounds, xml: shape });
                pads += usize::from(role == "copper");
                if size > MAX_BYTES {
                    return Err(invalid("SVG view exceeds 16 MiB"));
                }
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
        for layer in [ViewLayer::BottomCopper, ViewLayer::TopCopper, ViewLayer::BottomMask, ViewLayer::TopMask] {
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
    for (layer, color) in [
        (ViewLayer::Outline, "#d8ddd9"),
        (ViewLayer::BottomCopper, "#4773dc"),
        (ViewLayer::TopCopper, "#d65348"),
        (ViewLayer::BottomMask, "#02ffee"),
        (ViewLayer::TopMask, "#d864ff"),
        (ViewLayer::BottomPaste, "#508cb4"),
        (ViewLayer::TopPaste, "#8cb4c8"),
    ] {
        if !layers.contains(&layer) {
            continue;
        }
        writeln!(svg, "<g id=\"layer-{layer:?}\" fill=\"{color}\" color=\"{color}\">").unwrap();
        for shape in shapes.iter().filter(|s| s.layer == layer) {
            svg.push_str(&shape.xml);
        }
        svg.push_str("</g>\n");
    }
    if layers.iter().any(|l| {
        matches!(
            l,
            ViewLayer::TopSilkscreen | ViewLayer::BottomSilkscreen | ViewLayer::TopCourtyard | ViewLayer::BottomCourtyard | ViewLayer::NonplatedHoles
        )
    }) {
        svg.push_str("</g>\n");
        for (layer, color) in [
            (ViewLayer::BottomSilkscreen, "#e8b2a7"),
            (ViewLayer::TopSilkscreen, "#f2eda1"),
            (ViewLayer::BottomCourtyard, "#78a0d2"),
            (ViewLayer::TopCourtyard, "#c878d2"),
            (ViewLayer::NonplatedHoles, "#161a1e"),
        ] {
            if !layers.contains(&layer) {
                continue;
            }
            if layer == ViewLayer::NonplatedHoles {
                writeln!(svg, "<g id=\"layer-{layer:?}\" fill=\"{color}\" color=\"{color}\">").unwrap();
            } else {
                writeln!(svg, "<g id=\"layer-{layer:?}\" fill=\"none\" color=\"{color}\">").unwrap();
            }
            for shape in shapes.iter().filter(|s| s.layer == layer) {
                svg.push_str(&shape.xml);
            }
            svg.push_str("</g>\n");
        }
        svg.push_str("</g></svg>\n");
    } else {
        svg.push_str("</g></g></svg>\n");
    }
    if svg.len() > MAX_BYTES {
        return Err(invalid("SVG view exceeds 16 MiB"));
    }
    Ok(BoardSvg {
        svg,
        metadata: SvgMetadata {
            nonplated_holes: hole_metadata,
            pad_apertures: aperture_requested.then_some(ApertureViewMetadata{pads:aperture_rows,legacy_mask_policy:"Nominal copper-sized openings with zero expansion; native desktop legacy preview instead uses0.05mm border. No legacy paste inferred.",checks_not_performed:"No stencil, manufacturer, mask-sliver, silk-to-mask or fabrication suitability checks"}),
            artwork: artwork_metadata,
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
