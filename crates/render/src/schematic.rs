//! Bounded read-only selected-sheet views, with bundled glyph outlines.
use crate::schematic_labels::{LABEL_FONT_NM, LABEL_POLICY, LabelRole, SymbolLabel, schematic_label_layout};
use crate::{MAX_PNG_PIXELS, SvgOptions, ViewBounds};
use resvg::usvg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use skrifa::MetadataProvider;
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    io,
    sync::{Arc, OnceLock},
};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, LabelScope, SchematicSymbol},
};
use viamind_geom::Nm;

const MAX_TEXT: usize = 256 * 1024;
const MAX_GLYPHS: usize = 32768;
const MAX_SVG: usize = 16 * 1024 * 1024;
const FONT: &str = "Hack";
const FONT_NOTICE: &str = include_str!("../../../docs/licenses/Hack-Regular.txt");
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SchematicLayer {
    Buses,
    Wires,
    Symbols,
    Pins,
    Junctions,
    Labels,
}
fn layers() -> Vec<SchematicLayer> {
    vec![
        SchematicLayer::Buses,
        SchematicLayer::Wires,
        SchematicLayer::Symbols,
        SchematicLayer::Pins,
        SchematicLayer::Junctions,
        SchematicLayer::Labels,
    ]
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicViewOptions {
    #[serde(default = "layers")]
    pub layers: Vec<SchematicLayer>,
    #[serde(default)]
    pub region_nm: Option<ViewBounds>,
    #[serde(default = "padding")]
    pub padding_nm: i64,
    #[serde(default = "width")]
    pub width_px: u32,
    #[serde(default = "height")]
    pub height_px: u32,
}
fn padding() -> i64 {
    1_000_000
}
fn width() -> u32 {
    1600
}
fn height() -> u32 {
    1000
}
impl Default for SchematicViewOptions {
    fn default() -> Self {
        Self { layers: layers(), region_nm: None, padding_nm: padding(), width_px: width(), height_px: height() }
    }
}
impl SchematicViewOptions {
    pub fn validate(&self) -> io::Result<()> {
        SvgOptions {
            region_nm: self.region_nm,
            padding_nm: self.padding_nm,
            width_px: self.width_px,
            height_px: self.height_px,
            ..Default::default()
        }
        .validate()?;
        if self.layers.is_empty() || self.layers.len() > 32 {
            return Err(invalid("schematic layers requires 1–32 entries"));
        }
        if u64::from(self.width_px) * u64::from(self.height_px) > MAX_PNG_PIXELS {
            return Err(invalid("schematic viewport must not exceed 4194304 pixels"));
        }
        Ok(())
    }
}
/// Contains only selected-sheet data; construction checks bounds before cloning.
#[derive(Clone, Debug)]
pub struct SchematicSnapshot {
    project_id: ObjectId,
    project_name: String,
    sheet: Sheet,
}
pub fn schematic_view_snapshot(project: &Project, sheet_id: ObjectId) -> io::Result<SchematicSnapshot> {
    let sheet = project.schematic.sheets.get(&sheet_id).ok_or_else(|| invalid("schematic sheet does not exist"))?;
    if sheet.id != sheet_id || sheet.id.as_uuid().is_nil() {
        return Err(invalid("schematic sheet map key must match ID"));
    }
    validate_budget(&project.name, sheet)?;
    viamind_doc::schematic::validate_sheet(sheet).map_err(invalid)?;
    if project.id == sheet.id
        || sheet.content.as_ref().is_some_and(|c| {
            c.buses
                .keys()
                .chain(c.bus_entries.keys())
                .chain(c.packages.keys())
                .chain(c.symbols.keys())
                .chain(c.wires.keys())
                .chain(c.junctions.keys())
                .chain(c.labels.keys())
                .any(|id| *id == project.id)
                || c.symbols.values().flat_map(|s| &s.pins).any(|p| p.id == project.id)
        })
    {
        return Err(invalid("selected sheet identity collides with project"));
    }
    Ok(SchematicSnapshot { project_id: project.id, project_name: project.name.clone(), sheet: sheet.clone() })
}
pub(crate) fn validate_budget(project_name: &str, sheet: &Sheet) -> io::Result<()> {
    if let Some(c) = &sheet.content
        && (c.object_count() > 10_000 || c.pin_count() > 100_000)
    {
        return Err(invalid("selected sheet exceeds 10000 objects or 100000 pins"));
    }
    if let Some(c) = &sheet.content {
        viamind_doc::schematic_bus::budget(c).map_err(invalid)?;
        let (records, _) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
        if c.object_count().saturating_add(records.saturating_sub(c.packages.len())) > 10_000 {
            return Err(invalid("selected sheet exceeds 10000 objects including package membership records"));
        }
    }
    let mut bytes = 0usize;
    let mut glyphs = 0usize;
    let mut add = |s: &str, visible: bool| -> io::Result<()> {
        if s.len() > 4096 {
            return Err(invalid("schematic view strings exceed 4096 UTF-8 bytes"));
        }
        bytes = bytes.saturating_add(s.len());
        if bytes > MAX_TEXT {
            return Err(invalid("selected sheet view exceeds 256 KiB text"));
        }
        if visible {
            glyphs = glyphs.saturating_add(s.chars().count());
            if glyphs > MAX_GLYPHS {
                return Err(invalid("selected sheet view exceeds 32768 visible glyphs"));
            }
        }
        if s.chars().any(char::is_control) {
            return Err(invalid("schematic view text must not contain controls"));
        }
        Ok(())
    };
    add(project_name, false)?;
    add(&sheet.name, false)?;
    if let Some(c) = &sheet.content {
        for bus in c.buses.values() {
            add(&bus.expression, true)?;
        }
        for entry in c.bus_entries.values() {
            add(&entry.member, false)?;
        }
        for group in c.packages.values() {
            add(&group.template_name, false)?;
            for unit in &group.units {
                add(&unit.key, true)?;
                add(&unit.name, true)?;
                add(": ()", true)?;
            }
        }
        for s in c.symbols.values() {
            add(&s.designator, true)?;
            add(&s.value, true)?;
            if let Some(f) = &s.footprint {
                add(f, false)?;
            }
            for p in &s.pins {
                add(" ", true)?;
                add(&p.number, true)?;
                add(&p.name, true)?;
            }
        }
        for l in c.labels.values() {
            add(" (global)", true)?;
            add(&l.name, true)?;
        }
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct SchematicMetadata {
    pub project_id: ObjectId,
    pub sheet_id: ObjectId,
    pub sheet_name: String,
    pub region_nm: ViewBounds,
    pub width_px: u32,
    pub height_px: u32,
    pub layers: Vec<SchematicLayer>,
    pub units: &'static str,
    pub axes: &'static str,
    pub svg_coordinates: &'static str,
    pub font: &'static str,
    pub symbol_count: usize,
    pub pin_count: usize,
    pub bus_count: usize,
    pub bus_entry_count: usize,
    pub wire_count: usize,
    pub junction_count: usize,
    pub label_count: usize,
    pub legacy_empty_sheet: bool,
    pub exclusions: Vec<&'static str>,
    pub symbol_label_policy: &'static str,
    pub symbol_labels: Vec<SymbolLabel>,
}
#[derive(Debug, Serialize)]
pub struct SchematicSvg {
    pub svg: String,
    pub metadata: SchematicMetadata,
}
#[derive(Debug)]
pub struct SchematicPng {
    pub png: Vec<u8>,
    pub metadata: SchematicMetadata,
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}
fn fontdb() -> Arc<usvg::fontdb::Database> {
    static FONTDB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    Arc::clone(FONTDB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        Arc::new(db)
    }))
}
fn check_glyphs(text: &str) -> io::Result<()> {
    let font = skrifa::FontRef::new(epaint_default_fonts::HACK_REGULAR).map_err(io::Error::other)?;
    for c in text.chars() {
        if font.charmap().map(c).is_none() {
            return Err(invalid(format!("bundled Hack font does not cover U+{:04X}; use supported text", u32::from(c))));
        }
    }
    Ok(())
}
#[derive(Clone)]
enum Geometry {
    Line((i64, i64), (i64, i64), i64),
    Leader((i64, i64), (i64, i64)),
    Circle((i64, i64), i64, bool),
    Polygon(Vec<(i64, i64)>),
    Text((i64, i64), String, bool),
    SymbolLabel(Box<SymbolLabel>),
}
struct Shape {
    layer: SchematicLayer,
    id: String,
    color: &'static str,
    bounds: ViewBounds,
    geometry: Geometry,
}
fn point(p: Coordinate) -> (i64, i64) {
    (p.x.0, p.y.0)
}
fn bounds(a: (i64, i64), b: (i64, i64), r: i64) -> ViewBounds {
    ViewBounds { min_x: a.0.min(b.0) - r, min_y: a.1.min(b.1) - r, max_x: a.0.max(b.0) + r, max_y: a.1.max(b.1) + r }
}
fn transform(s: &SchematicSymbol, x: i64, y: i64) -> io::Result<(i64, i64)> {
    s.pin_position(Coordinate { x: Nm(x), y: Nm(y) }).map(point).map_err(invalid)
}
fn union(a: ViewBounds, b: ViewBounds) -> ViewBounds {
    ViewBounds { min_x: a.min_x.min(b.min_x), min_y: a.min_y.min(b.min_y), max_x: a.max_x.max(b.max_x), max_y: a.max_y.max(b.max_y) }
}
fn text_shape(layer: SchematicLayer, id: String, color: &'static str, at: (i64, i64), text: String, center: bool) -> io::Result<Shape> {
    check_glyphs(&text)?;
    // Hack's monospace advance is <0.7 em; two-em vertical envelope also contains accents/descenders.
    let width = i64::try_from(text.chars().count()).unwrap_or(i64::MAX).saturating_mul(700_000);
    let start = if center { at.0 - width / 2 } else { at.0 };
    Ok(Shape {
        layer,
        id,
        color,
        bounds: ViewBounds { min_x: start - 200_000, min_y: at.1 - 1_500_000, max_x: start + width + 200_000, max_y: at.1 + 500_000 },
        geometry: Geometry::Text(at, text, center),
    })
}
fn shapes(snapshot: &SchematicSnapshot, selected: &BTreeSet<SchematicLayer>, labels: &[SymbolLabel]) -> io::Result<Vec<Shape>> {
    let mut shapes = Vec::new();
    let Some(c) = &snapshot.sheet.content else {
        return Ok(shapes);
    };
    let wire = "#237347";
    let symbol = "#806215";
    let label = "#275d9d";
    if selected.contains(&SchematicLayer::Buses) {
        for b in c.buses.values() {
            let (a, z) = (point(b.start), point(b.end));
            shapes.push(Shape {
                layer: SchematicLayer::Buses,
                id: format!("bus-{}", b.id),
                color: "#497cc2",
                bounds: bounds(a, z, 200_000),
                geometry: Geometry::Line(a, z, 400_000),
            });
            shapes.push(text_shape(
                SchematicLayer::Buses,
                format!("bus-expression-{}", b.id),
                "#497cc2",
                (a.0, a.1 - 500_000),
                b.expression.clone(),
                false,
            )?);
        }
        for e in c.bus_entries.values() {
            let (a, z) = (point(e.bus_point), point(e.wire_point));
            shapes.push(Shape {
                layer: SchematicLayer::Buses,
                id: format!("bus-entry-{}", e.id),
                color: "#497cc2",
                bounds: bounds(a, z, 125_000),
                geometry: Geometry::Line(a, z, 250_000),
            });
        }
    }
    if selected.contains(&SchematicLayer::Wires) {
        for w in c.wires.values() {
            let (a, b) = (point(w.start), point(w.end));
            shapes.push(Shape {
                layer: SchematicLayer::Wires,
                id: format!("wire-{}", w.id),
                color: wire,
                bounds: bounds(a, b, 75_000),
                geometry: Geometry::Line(a, b, 150_000),
            });
        }
    }
    for s in c.symbols.values() {
        if selected.contains(&SchematicLayer::Symbols) {
            let (x, y) = (s.width.0 / 2, s.height.0 / 2);
            let corners = [(-x, -y), (s.width.0 - x, -y), (s.width.0 - x, s.height.0 - y), (-x, s.height.0 - y)]
                .into_iter()
                .map(|(x, y)| transform(s, x, y))
                .collect::<io::Result<Vec<_>>>()?;
            let b = corners.iter().map(|p| bounds(*p, *p, 75_000)).reduce(union).expect("four corners");
            shapes.push(Shape {
                layer: SchematicLayer::Symbols,
                id: format!("symbol-{}", s.id),
                color: symbol,
                bounds: b,
                geometry: Geometry::Polygon(corners),
            });
        }
        if selected.contains(&SchematicLayer::Pins) {
            for p in &s.pins {
                let at = point(s.pin_position(p.position).map_err(invalid)?);
                let start = transform(
                    s,
                    p.position.x.0.clamp(-s.width.0 / 2, s.width.0 - s.width.0 / 2),
                    p.position.y.0.clamp(-s.height.0 / 2, s.height.0 - s.height.0 / 2),
                )?;
                shapes.push(Shape {
                    layer: SchematicLayer::Pins,
                    id: format!("pin-stem-{}", p.id),
                    color: symbol,
                    bounds: bounds(start, at, 50_000),
                    geometry: Geometry::Line(start, at, 100_000),
                });
                shapes.push(Shape {
                    layer: SchematicLayer::Pins,
                    id: format!("pin-{}", p.id),
                    color: symbol,
                    bounds: bounds(at, at, 200_000),
                    geometry: Geometry::Circle(at, 150_000, false),
                });
            }
        }
    }
    for label in labels {
        check_glyphs(&label.text)?;
        let layer = if label.role == LabelRole::Pin { SchematicLayer::Pins } else { SchematicLayer::Symbols };
        if let Some([start, end]) = label.leader {
            shapes.push(Shape {
                layer,
                id: format!("pin-leader-{}", label.object_id),
                color: symbol,
                bounds: bounds(point(start), point(end), 50_000),
                geometry: Geometry::Leader(point(start), point(end)),
            });
        }
        shapes.push(Shape {
            layer,
            id: label.id.clone(),
            color: symbol,
            bounds: label.envelope,
            geometry: Geometry::SymbolLabel(Box::new(label.clone())),
        });
    }
    if selected.contains(&SchematicLayer::Junctions) {
        for j in c.junctions.values() {
            let at = point(j.position);
            shapes.push(Shape {
                layer: SchematicLayer::Junctions,
                id: format!("junction-{}", j.id),
                color: wire,
                bounds: bounds(at, at, 200_000),
                geometry: Geometry::Circle(at, 200_000, true),
            });
        }
    }
    if selected.contains(&SchematicLayer::Labels) {
        for l in c.labels.values() {
            let at = point(l.position);
            let scope = match l.scope {
                LabelScope::Local => "local",
                LabelScope::Global => "global",
                LabelScope::Power => "power",
            };
            shapes.push(Shape {
                layer: SchematicLayer::Labels,
                id: format!("label-anchor-{}", l.id),
                color: label,
                bounds: bounds(at, at, 200_000),
                geometry: Geometry::Circle(at, 150_000, false),
            });
            shapes.push(text_shape(
                SchematicLayer::Labels,
                format!("label-{}", l.id),
                label,
                (at.0 + 250_000, at.1 - 1_200_000),
                format!("{} ({scope})", l.name),
                false,
            )?);
        }
    }
    shapes.sort_by_key(|s| s.layer);
    Ok(shapes)
}
pub fn schematic_svg(snapshot: &SchematicSnapshot, options: &SchematicViewOptions) -> io::Result<SchematicSvg> {
    options.validate()?;
    let selected: BTreeSet<_> = options.layers.iter().copied().collect();
    let labels: Vec<_> = if selected.contains(&SchematicLayer::Symbols) || selected.contains(&SchematicLayer::Pins) {
        schematic_label_layout(&snapshot.sheet)?
            .into_iter()
            .filter(|label| selected.contains(&if label.role == LabelRole::Pin { SchematicLayer::Pins } else { SchematicLayer::Symbols }))
            .collect()
    } else {
        vec![]
    };
    let shapes = shapes(snapshot, &selected, &labels)?;
    let region = options.region_nm.unwrap_or_else(|| {
        let b = shapes.iter().map(|s| s.bounds).reduce(union).unwrap_or(ViewBounds {
            min_x: -5_000_000,
            min_y: -5_000_000,
            max_x: 5_000_000,
            max_y: 5_000_000,
        });
        ViewBounds {
            min_x: b.min_x - options.padding_nm,
            min_y: b.min_y - options.padding_nm,
            max_x: b.max_x + options.padding_nm,
            max_y: b.max_y + options.padding_nm,
        }
    });
    SvgOptions { region_nm: Some(region), ..Default::default() }.validate()?;
    let scale =
        (f64::from(options.width_px) / (region.max_x - region.min_x) as f64).min(f64::from(options.height_px) / (region.max_y - region.min_y) as f64);
    let offset = (
        (f64::from(options.width_px) - (region.max_x - region.min_x) as f64 * scale) / 2.0,
        (f64::from(options.height_px) - (region.max_y - region.min_y) as f64 * scale) / 2.0,
    );
    let pos = |p: (i64, i64)| ((p.0 - region.min_x) as f64 * scale + offset.0, (p.1 - region.min_y) as f64 * scale + offset.1);
    let mut source = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><defs><clipPath id=\"region-clip\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#region-clip)\">",
        options.width_px,
        options.height_px,
        options.width_px,
        options.height_px,
        offset.0,
        offset.1,
        (region.max_x - region.min_x) as f64 * scale,
        (region.max_y - region.min_y) as f64 * scale
    );
    let mut semantic = Vec::new();
    for s in shapes {
        let b = s.bounds;
        if b.max_x < region.min_x || b.min_x > region.max_x || b.max_y < region.min_y || b.min_y > region.max_y {
            continue;
        }
        let magnitude =
            [pos((b.min_x, b.min_y)).0.abs(), pos((b.min_x, b.min_y)).1.abs(), pos((b.max_x, b.max_y)).0.abs(), pos((b.max_x, b.max_y)).1.abs()]
                .into_iter()
                .fold(1.0, f64::max);
        if magnitude * f64::from(f32::EPSILON) * 4.0 > 0.125 {
            return Err(invalid("schematic region exceeds 1/8-pixel precision budget; widen the region"));
        }
        let id = &s.id;
        let color = s.color;
        match s.geometry {
            Geometry::Leader(a, b) => {
                let (a, b) = (pos(a), pos(b));
                write!(
                    source,
                    "<path id=\"{id}\" d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-dasharray=\"{} {}\"/>",
                    a.0,
                    a.1,
                    b.0,
                    b.1,
                    50_000.0 * scale,
                    200_000.0 * scale,
                    150_000.0 * scale
                )
                .unwrap();
            }
            Geometry::Line(a, b, w) => {
                let (a, b) = (pos(a), pos(b));
                write!(
                    source,
                    "<path id=\"{id}\" d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                    a.0,
                    a.1,
                    b.0,
                    b.1,
                    w as f64 * scale
                )
                .unwrap();
            }
            Geometry::Polygon(points) => {
                write!(source, "<path id=\"{id}\" d=\"").unwrap();
                for (i, p) in points.into_iter().enumerate() {
                    let p = pos(p);
                    write!(source, "{} {} {} ", if i == 0 { "M" } else { "L" }, p.0, p.1).unwrap();
                }
                write!(source, "Z\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>", 150_000.0 * scale).unwrap();
            }
            Geometry::Circle(p, r, filled) => {
                let p = pos(p);
                write!(
                    source,
                    "<circle id=\"{id}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{color}\" stroke-width=\"{}\"/>",
                    p.0,
                    p.1,
                    r as f64 * scale,
                    if filled { color } else { "none" },
                    if filled { 0.0 } else { 100_000.0 * scale }
                )
                .unwrap();
            }
            Geometry::Text(p, text, center) => {
                let p = pos(p);
                write!(
                    source,
                    "<text id=\"{id}\" x=\"{}\" y=\"{}\" font-family=\"Hack\" font-size=\"{}\" text-anchor=\"{}\" fill=\"{color}\">{}</text>",
                    p.0,
                    p.1,
                    1_000_000.0 * scale,
                    if center { "middle" } else { "start" },
                    xml(&text)
                )
                .unwrap();
                semantic.push(serde_json::json!({"id":id,"text":text}));
            }
            Geometry::SymbolLabel(label) => {
                let p = pos((label.anchor.x.0, label.anchor.y.0 + LABEL_FONT_NM));
                write!(
                    source,
                    "<text id=\"{id}\" x=\"{}\" y=\"{}\" font-family=\"Hack\" font-size=\"{}\" text-anchor=\"start\" fill=\"{color}\">{}</text>",
                    p.0,
                    p.1,
                    LABEL_FONT_NM as f64 * scale,
                    xml(&label.display_text)
                )
                .unwrap();
                semantic.push(serde_json::to_value(label).map_err(io::Error::other)?);
            }
        }
        if source.len() > MAX_SVG {
            return Err(invalid("schematic SVG exceeds 16 MiB"));
        }
    }
    source.push_str("</g></svg>");
    let parsing = usvg::Options {
        font_family: FONT.into(),
        fontdb: fontdb(),
        image_href_resolver: usvg::ImageHrefResolver { resolve_data: Box::new(|_, _, _| None), resolve_string: Box::new(|_, _| None) },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&source, &parsing).map_err(io::Error::other)?;
    let mut svg = tree.to_string(&usvg::WriteOptions::default());
    svg.insert_str(4, &format!(" viewBox=\"0 0 {} {}\"", options.width_px, options.height_px));
    let c = snapshot.sheet.content.as_ref();
    let metadata = SchematicMetadata {
        project_id: snapshot.project_id,
        sheet_id: snapshot.sheet.id,
        sheet_name: snapshot.sheet.name.clone(),
        region_nm: region,
        width_px: options.width_px,
        height_px: options.height_px,
        layers: selected.into_iter().collect(),
        units: "model nanometres; SVG viewport pixels",
        axes: "X right, Y down; mirrored local X then CCW quarter turns",
        svg_coordinates: "0 0 width_px height_px; uniformly fit region_nm, centered; clip to region",
        font: "bundled unchanged Hack Regular; visible glyphs outlined",
        symbol_count: c.map_or(0, |c| c.symbols.len()),
        pin_count: c.map_or(0, viamind_doc::schematic::SchematicContent::pin_count),
        bus_count: c.map_or(0, |c| c.buses.len()),
        bus_entry_count: c.map_or(0, |c| c.bus_entries.len()),
        wire_count: c.map_or(0, |c| c.wires.len()),
        junction_count: c.map_or(0, |c| c.junctions.len()),
        label_count: c.map_or(0, |c| c.labels.len()),
        legacy_empty_sheet: c.is_none(),
        exclusions: vec![
            "board geometry",
            "other sheets",
            "hierarchy",
            "bus and entry graphics do not conduct; scalar wires and labels supply connectivity",
            "custom symbol artwork",
            "electrical certification",
            "global label collision avoidance",
        ],
        symbol_label_policy: LABEL_POLICY,
        symbol_labels: labels,
    };
    let info = serde_json::json!({"view":metadata,"project_name":snapshot.project_name,"visible_text":semantic,"font_notice":FONT_NOTICE});
    let at = svg.find('>').ok_or_else(|| io::Error::other("outlined SVG root missing"))? + 1;
    svg.insert_str(
        at,
        &format!(
            "<title>{}</title><metadata>{}</metadata>",
            xml(&format!("{} — {}", snapshot.project_name, snapshot.sheet.name)),
            xml(&info.to_string())
        ),
    );
    if svg.len() > MAX_SVG {
        return Err(invalid("outlined schematic SVG exceeds 16 MiB"));
    }
    Ok(SchematicSvg { svg, metadata })
}
pub fn schematic_png(snapshot: &SchematicSnapshot, options: &SchematicViewOptions) -> io::Result<SchematicPng> {
    let rendered = schematic_svg(snapshot, options)?;
    let png = crate::png::rasterize_svg(&rendered.svg, options.width_px, options.height_px)?;
    Ok(SchematicPng { png, metadata: rendered.metadata })
}
