//! Independent bounded standalone PCB S-expression conversion; see the public-source spec.
use crate::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use viamind_geom::{Nm, Point, Polygon};

pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 20_000;
const MAX_TOKENS: usize = 100_000;
const MAX_OBJECTS: usize = 10_000;
const MAX_STRING: usize = 4096;
const MAX_TEXT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Imported,
    Omitted,
    Blocked,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FidelityItem {
    pub path: String,
    pub byte_offset: usize,
    pub token: String,
    pub source_id: Option<ObjectId>,
    pub disposition: Disposition,
    pub reason: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImportedCounts {
    pub parts: usize,
    pub pads: usize,
    pub tracks: usize,
    pub vias: usize,
    pub outline_vertices: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FidelityReport {
    pub format_version: String,
    pub imported: ImportedCounts,
    pub omitted_lists: usize,
    pub blocked_lists: usize,
    pub full_fidelity: bool,
    pub items: Vec<FidelityItem>,
}
impl FidelityReport {
    pub fn can_import(&self) -> bool {
        self.blocked_lists == 0
    }
}
#[derive(Clone, Debug)]
pub struct ParsedPcb {
    pub project: Project,
    pub report: FidelityReport,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("PCB import: {0}")]
pub struct ImportError(pub String);
fn bad(message: impl Into<String>) -> ImportError {
    ImportError(message.into())
}
type Result<T> = std::result::Result<T, ImportError>;

pub(crate) struct Node {
    pub(crate) id: usize,
    pub(crate) head: String,
    pub(crate) args: Vec<String>,
    pub(crate) children: Vec<Node>,
}
struct Parser<'a> {
    source: &'a str,
    at: usize,
    tokens: usize,
    text: usize,
    items: Vec<FidelityItem>,
}
impl Parser<'_> {
    fn skip(&mut self) {
        loop {
            while self.source.as_bytes().get(self.at).is_some_and(u8::is_ascii_whitespace) {
                self.at += 1;
            }
            if self.source.as_bytes().get(self.at) != Some(&b';') {
                break;
            }
            while self.source.as_bytes().get(self.at).is_some_and(|b| *b != b'\n') {
                self.at += 1;
            }
        }
    }
    fn token(&mut self) -> Result<String> {
        self.skip();
        if self.tokens >= MAX_TOKENS {
            return Err(bad("more than 100000 tokens"));
        }
        self.tokens += 1;
        let start = self.at;
        let quoted = self.source.as_bytes().get(self.at) == Some(&b'"');
        if quoted {
            self.at += 1;
        }
        let mut value = String::new();
        loop {
            let Some(c) = self.source[self.at..].chars().next() else {
                if quoted {
                    return Err(bad(format!("unterminated string at byte {start}")));
                }
                break;
            };
            if quoted && c == '"' {
                self.at += 1;
                break;
            }
            if !quoted && (c.is_ascii_whitespace() || matches!(c, '(' | ')' | ';')) {
                break;
            }
            self.at += c.len_utf8();
            let c = if quoted && c == '\\' {
                let escaped = self.source[self.at..].chars().next().ok_or_else(|| bad("truncated string escape"))?;
                self.at += escaped.len_utf8();
                match escaped {
                    '"' => '"',
                    '\\' => '\\',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => return Err(bad(format!("unsupported string escape at byte {}", self.at))),
                }
            } else {
                c
            };
            if c == '\0' {
                return Err(bad("NUL/binary input is unsupported"));
            }
            if value.len() + c.len_utf8() > MAX_STRING || self.text + c.len_utf8() > MAX_TEXT {
                return Err(bad("string/text exceeds 4096-byte individual or 2 MiB aggregate limit"));
            }
            value.push(c);
            self.text += c.len_utf8();
        }
        if self.at == start {
            return Err(bad(format!("expected atom at byte {start}")));
        }
        Ok(value)
    }
    fn list(&mut self, path: String, depth: usize) -> Result<Node> {
        self.skip();
        if depth > 32 || self.items.len() >= MAX_NODES {
            return Err(bad("list depth exceeds 32 or object/report count exceeds 20000"));
        }
        if self.source.as_bytes().get(self.at) != Some(&b'(') {
            return Err(bad(format!("expected '(' at byte {}", self.at)));
        }
        let offset = self.at;
        self.at += 1;
        let head = self.token()?;
        if head.is_empty() {
            return Err(bad("empty list head"));
        }
        let id = self.items.len();
        self.items.push(FidelityItem {
            path: path.clone(),
            byte_offset: offset,
            token: head.clone(),
            source_id: None,
            disposition: Disposition::Blocked,
            reason: "Unsupported object or attribute; export the documented supported geometry subset.".into(),
        });
        let mut node = Node { id, head, args: Vec::new(), children: Vec::new() };
        loop {
            self.skip();
            match self.source.as_bytes().get(self.at) {
                Some(b')') => {
                    self.at += 1;
                    return Ok(node);
                }
                Some(b'(') => node.children.push(self.list(format!("{path}/{}", node.children.len()), depth + 1)?),
                None => return Err(bad(format!("unclosed list at byte {offset}"))),
                _ => node.args.push(self.token()?),
            }
        }
    }
}
pub(crate) fn mark(items: &mut [FidelityItem], n: &Node, disposition: Disposition, reason: &str) {
    items[n.id].disposition = disposition;
    items[n.id].reason = reason.into();
}
pub(crate) fn subtree(items: &mut [FidelityItem], n: &Node, disposition: Disposition, reason: &str) {
    mark(items, n, disposition, reason);
    for child in &n.children {
        subtree(items, child, disposition, reason);
    }
}
pub(crate) fn child<'a>(n: &'a Node, name: &str) -> Result<Option<&'a Node>> {
    let mut values = n.children.iter().filter(|n| n.head == name);
    let result = values.next();
    if values.next().is_some() {
        return Err(bad(format!("duplicate {name} field")));
    }
    Ok(result)
}
pub(crate) fn atoms(n: &Node, count: usize) -> Result<&[String]> {
    if n.args.len() != count || !n.children.is_empty() {
        return Err(bad(format!("{} requires exactly {count} scalar values", n.head)));
    }
    Ok(&n.args)
}
pub(crate) fn field<'a>(n: &'a Node, name: &str, count: usize, items: &mut [FidelityItem]) -> Result<&'a [String]> {
    let c = child(n, name)?.ok_or_else(|| bad(format!("missing {name}")))?;
    let a = atoms(c, count)?;
    mark(items, c, Disposition::Imported, "Exact native field.");
    Ok(a)
}
pub(crate) fn allowed(n: &Node, fields: &[&str], ignored: &[&str], items: &mut [FidelityItem]) -> Result<()> {
    for c in &n.children {
        if ignored.contains(&c.head.as_str()) {
            subtree(items, c, Disposition::Omitted, "Presentation/metadata attribute is not persisted; no manufacturing equivalence implied.");
        } else if !fields.contains(&c.head.as_str()) {
            return Err(bad(format!("unsupported semantic attribute at {}; export the documented supported subset", items[c.id].path)));
        }
    }
    Ok(())
}
/// Recognized ignored scalar fields must not hide nested semantics or duplicates.
pub(crate) fn omit_scalars(n: &Node, fields: &[&str], items: &mut [FidelityItem]) -> Result<()> {
    for field in fields {
        if let Some(c) = child(n, field)? {
            atoms(c, 1)?;
            mark(items, c, Disposition::Omitted, "Recognized scalar pin metadata is not retained; no electrical semantics inferred.");
        }
    }
    Ok(())
}
/// Convert exact decimal millimetres without floating-point rounding or precision loss.
pub fn decimal_nm(s: &str) -> Result<Nm> {
    let (negative, s) = if let Some(s) = s.strip_prefix('-') { (true, s) } else { (false, s.strip_prefix('+').unwrap_or(s)) };
    let mut p = s.split('.');
    let whole = p.next().unwrap_or_default();
    let fraction = p.next().unwrap_or_default();
    if p.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad("expected decimal millimetres without exponent"));
    }
    if fraction.bytes().skip(6).any(|b| b != b'0') {
        return Err(bad("nonzero sub-nanometre precision is unsupported; export exact Nm geometry"));
    }
    let whole = whole.parse::<i64>().map_err(|_| bad("coordinate/dimension overflow"))?;
    let frac = fraction.bytes().take(6).fold(0_i64, |n, b| n * 10 + i64::from(b - b'0'));
    let value = whole
        .checked_mul(1_000_000)
        .and_then(|n| n.checked_add(frac * 10_i64.pow((6 - fraction.len().min(6)) as u32)))
        .ok_or_else(|| bad("coordinate/dimension overflow"))?;
    if value > 10_000_000_000 {
        return Err(bad("coordinates/dimensions exceed 10000 mm"));
    }
    Ok(Nm(if negative { -value } else { value }))
}
fn point(n: &Node, name: &str, items: &mut [FidelityItem]) -> Result<Point> {
    let a = field(n, name, 2, items)?;
    Ok(Point::new(decimal_nm(&a[0])?, decimal_nm(&a[1])?))
}
pub(crate) fn at_zero(n: &Node, items: &mut [FidelityItem]) -> Result<Point> {
    let c = child(n, "at")?.ok_or_else(|| bad("missing at"))?;
    if !c.children.is_empty() || !(2..=3).contains(&c.args.len()) {
        return Err(bad("at requires X Y and optional zero rotation"));
    }
    if c.args.get(2).is_some_and(|a| decimal_nm(a).map_or(true, |v| v.0 != 0)) {
        return Err(bad("only zero-angle containing footprint placement is supported"));
    }
    mark(items, c, Disposition::Imported, "Exact zero-angle containing-object coordinates.");
    Ok(Point::new(decimal_nm(&c.args[0])?, decimal_nm(&c.args[1])?))
}
/// Parse exact cardinal degrees without interpreting them as lengths or using floats.
fn cardinal_degrees(token: &str) -> Result<i32> {
    let error = || bad("pad angle must be an exact multiple of 90 degrees within -360..360; use a bounded integer decimal without exponent");
    if token.len() > MAX_STRING {
        return Err(error());
    }
    let (negative, unsigned) = token.strip_prefix('-').map_or((false, token), |s| (true, s));
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b == b'0'))
        || parts.next().is_some()
    {
        return Err(error());
    }
    let whole = whole.parse::<i32>().map_err(|_| error())?;
    if whole > 360 || whole % 90 != 0 {
        return Err(error());
    }
    Ok(if negative { -whole } else { whole })
}
/// Represent cardinal rectangular copper exactly in the native axis-aligned pad model.
/// The containing footprint remains front/zero-angle; no combined transform is inferred.
pub(crate) fn cardinal_pad_geometry(n: &Node, items: &mut [FidelityItem]) -> Result<(Point, Nm, Nm)> {
    let at = child(n, "at")?.ok_or_else(|| bad("missing pad at"))?;
    if !at.children.is_empty() || !(2..=3).contains(&at.args.len()) {
        return Err(bad("pad at requires X Y and optional exact cardinal angle within -360..360 degrees"));
    }
    let source_angle = at.args.get(2).map_or(Ok(0), |a| cardinal_degrees(a))?;
    let canonical = source_angle.rem_euclid(360);
    let swapped = matches!(canonical, 90 | 270);
    let position = Point::new(decimal_nm(&at.args[0])?, decimal_nm(&at.args[1])?);
    let size = field(n, "size", 2, items)?;
    let (width, height) = (decimal_nm(&size[0])?, decimal_nm(&size[1])?);
    let dimensions = if swapped { "width and height swapped exactly" } else { "width and height preserved exactly" };
    mark(
        items,
        at,
        Disposition::Imported,
        &format!(
            "Exact rectangular copper normalization: source angle {source_angle} degrees, canonical {canonical} degrees; center unchanged; {dimensions}. Native pad has no angle field."
        ),
    );
    let size_node = child(n, "size")?.expect("validated size field");
    mark(
        items,
        size_node,
        Disposition::Imported,
        &format!("Exact rectangle dimensions for canonical {canonical} degrees: {dimensions}; no rounding."),
    );
    Ok((position, if swapped { height } else { width }, if swapped { width } else { height }))
}
fn identity(n: &Node, items: &mut [FidelityItem]) -> Result<ObjectId> {
    let a = child(n, "uuid")?;
    let b = child(n, "tstamp")?;
    let ((Some(c), None) | (None, Some(c))) = (a, b) else {
        return Err(bad("exactly one nonnil uuid/tstamp is required"));
    };
    let id = atoms(c, 1)?[0].parse::<ObjectId>().map_err(|_| bad("invalid UUID"))?;
    mark(items, c, Disposition::Imported, "Stable source identity retained in native object.");
    items[n.id].source_id = Some(id);
    Ok(id)
}
pub(crate) fn name(s: &str) -> Result<String> {
    if s.is_empty() || s.trim() != s || s.chars().count() > 128 || s.chars().any(char::is_control) {
        return Err(bad("name/number must be trimmed, nonempty, control-free and at most 128 characters"));
    }
    Ok(s.into())
}
fn net(n: &Node, table: &BTreeMap<u32, String>, items: &mut [FidelityItem]) -> Result<Option<String>> {
    let Some(c) = child(n, "net")? else {
        return Ok(None);
    };
    if !c.children.is_empty() || !(1..=2).contains(&c.args.len()) {
        return Err(bad("net needs ordinal and optional matching literal name"));
    }
    let ordinal = c.args[0].parse::<u32>().map_err(|_| bad("invalid net ordinal"))?;
    let literal = table.get(&ordinal).ok_or_else(|| bad("net references undefined ordinal"))?;
    if c.args.get(1).is_some_and(|s| s != literal) {
        return Err(bad("pad net literal conflicts with net table"));
    }
    mark(items, c, Disposition::Imported, "Exact net-table assignment; ordinal zero is unassigned.");
    Ok(if ordinal == 0 { None } else { Some(literal.clone()) })
}
fn layer(n: &Node, items: &mut [FidelityItem]) -> Result<CopperLayer> {
    match field(n, "layer", 1, items)?[0].as_str() {
        "F.Cu" => Ok(CopperLayer::Top),
        "B.Cu" => Ok(CopperLayer::Bottom),
        _ => Err(bad("only canonical F.Cu/B.Cu copper is supported")),
    }
}
fn track(n: &Node, table: &BTreeMap<u32, String>, items: &mut [FidelityItem]) -> Result<Track> {
    if !n.args.is_empty() {
        return Err(bad("unsupported segment flags"));
    }
    allowed(n, &["start", "end", "width", "layer", "net", "uuid", "tstamp"], &[], items)?;
    let t = Track {
        id: identity(n, items)?,
        start: point(n, "start", items)?,
        end: point(n, "end", items)?,
        width: decimal_nm(&field(n, "width", 1, items)?[0])?,
        layer: layer(n, items)?,
        net: net(n, table, items)?,
    };
    t.validate().map_err(bad)?;
    Ok(t)
}
fn via(n: &Node, table: &BTreeMap<u32, String>, items: &mut [FidelityItem]) -> Result<Via> {
    if !n.args.is_empty() {
        return Err(bad("only default circular through vias without flags are supported"));
    }
    allowed(n, &["at", "size", "drill", "layers", "net", "uuid", "tstamp"], &[], items)?;
    let layers = field(n, "layers", 2, items)?;
    if layers != ["F.Cu", "B.Cu"] && layers != ["B.Cu", "F.Cu"] {
        return Err(bad("via must span F.Cu and B.Cu"));
    }
    let v = Via {
        id: identity(n, items)?,
        position: point(n, "at", items)?,
        diameter: decimal_nm(&field(n, "size", 1, items)?[0])?,
        drill: decimal_nm(&field(n, "drill", 1, items)?[0])?,
        net: net(n, table, items)?,
    };
    v.validate().map_err(bad)?;
    Ok(v)
}
fn pad(n: &Node, table: &BTreeMap<u32, String>, items: &mut [FidelityItem]) -> Result<Pad> {
    if n.args.len() != 3 || n.args[1] != "smd" || n.args[2] != "rect" {
        return Err(bad("only explicit rectangular smd pads are supported"));
    }
    allowed(n, &["at", "size", "layers", "net", "uuid", "tstamp", "pinfunction", "pintype"], &[], items)?;
    omit_scalars(n, &["pinfunction", "pintype"], items)?;
    let l = child(n, "layers")?.ok_or_else(|| bad("missing pad layers"))?;
    if !l.children.is_empty()
        || !l.args.iter().any(|s| s == "F.Cu")
        || l.args.iter().any(|s| !matches!(s.as_str(), "F.Cu" | "F.Mask" | "F.Paste"))
        || l.args.iter().collect::<BTreeSet<_>>().len() != l.args.len()
    {
        return Err(bad("front pads support F.Cu with optional F.Mask/F.Paste only"));
    }
    mark(
        items,
        l,
        if l.args.len() == 1 { Disposition::Imported } else { Disposition::Omitted },
        "Front copper imported; mask/paste layers are not represented by the native pad model.",
    );
    let (position, width, height) = cardinal_pad_geometry(n, items)?;
    Ok(Pad { id: identity(n, items)?, number: name(&n.args[0])?, position, width, height, net: net(n, table, items)?, apertures: None })
}
fn footprint(n: &Node, table: &BTreeMap<u32, String>, items: &mut [FidelityItem]) -> Result<Part> {
    if n.args.len() != 1 {
        return Err(bad("footprint requires one library name without flags"));
    }
    allowed(
        n,
        &["at", "layer", "uuid", "tstamp", "pad", "property", "fp_text", "attr"],
        &[
            "descr",
            "tags",
            "path",
            "sheetname",
            "sheetfile",
            "tedit",
            "model",
            "fp_line",
            "fp_rect",
            "fp_circle",
            "fp_arc",
            "fp_poly",
            "fp_curve",
            "fp_text_box",
        ],
        items,
    )?;
    // Any rendered footprint field on copper/outline affects physical geometry.
    for c in &n.children {
        if c.head.starts_with("fp_") || c.head == "property" {
            let l = child(c, "layer")?;
            if l.is_some_and(|l| l.args.iter().any(|s| s.ends_with(".Cu") || s == "Edge.Cuts")) {
                return Err(bad("footprint copper/outline graphics or text are unsupported"));
            }
            if c.head.starts_with("fp_") && l.is_none() {
                return Err(bad("footprint graphic requires an explicit non-copper layer"));
            }
        }
    }
    if layer(n, items)? != CopperLayer::Top {
        return Err(bad("only front-side footprints are supported"));
    }
    let mut reference = None;
    let mut value = None;
    for c in &n.children {
        if matches!(c.head.as_str(), "property" | "fp_text") {
            if (c.head == "property" && c.args.len() != 2)
                || (c.head == "fp_text" && (c.args.len() < 2 || c.args.len() > 3 || c.args.get(2).is_some_and(|a| a != "hide")))
            {
                return Err(bad("property requires exactly key/value; fp_text permits only type/text and optional hide"));
            }
            let key = c.args[0].to_ascii_lowercase();
            let target = match key.as_str() {
                "reference" => Some(&mut reference),
                "value" => Some(&mut value),
                _ => None,
            };
            if let Some(target) = target {
                if target.as_ref().is_some_and(|v| v != &c.args[1]) {
                    return Err(bad("conflicting reference/value fields"));
                }
                *target = Some(c.args[1].clone());
            }
            subtree(
                items,
                c,
                Disposition::Omitted,
                "Reference/value text content retained when recognized; presentation and other properties are not persisted.",
            );
        }
        if c.head == "attr" {
            if !c.children.is_empty() || c.args != ["smd"] {
                return Err(bad("only unqualified smd footprint attr is supported"));
            }
            mark(items, c, Disposition::Imported, "SMD component type represented by explicit native pads.");
        }
    }
    let mut pads = Vec::new();
    for c in n.children.iter().filter(|c| c.head == "pad") {
        if pads.len() >= 1024 {
            return Err(bad("more than 1024 pads per footprint"));
        }
        let p = pad(c, table, items)?;
        mark(items, c, Disposition::Imported, "Exact rectangular native SMD pad.");
        pads.push(p);
    }
    if pads.is_empty() {
        return Err(bad("footprint has no explicit supported pads"));
    }
    let p = Part {
        holes: None,
        artwork: None,
        assembly: None,
        id: identity(n, items)?,
        schematic_symbol: None,
        schematic_package: None,
        pads: Some(Pads::try_from(pads).map_err(bad)?),
        designator: name(&reference.ok_or_else(|| bad("missing explicit Reference"))?)?,
        value: value.unwrap_or_default(),
        footprint: name(&n.args[0])?,
        position: at_zero(n, items)?,
        rotation_mdeg: 0,
        side: Side::Top,
    };
    for pad in &p.pads.as_ref().unwrap().0 {
        let world = p.local_to_world_nm_exact(pad.position).map_err(bad)?;
        if [world.x.0, world.y.0].iter().any(|v| v.unsigned_abs() > 10_000_000_000) {
            return Err(bad("transformed pad center exceeds 10000 mm"));
        }
    }
    Ok(p)
}
fn outline_edges(n: &Node, items: &mut [FidelityItem]) -> Result<Vec<(Point, Point)>> {
    if !n.args.is_empty() {
        return Err(bad("unsupported outline flags"));
    }
    allowed(n, &["start", "end", "layer", "uuid", "tstamp"], &["stroke", "width", "fill"], items)?;
    if field(n, "layer", 1, items)?[0] != "Edge.Cuts" {
        return Err(bad("outline must use canonical Edge.Cuts"));
    }
    identity(n, items)?;
    for field in n.children.iter().filter(|c| matches!(c.head.as_str(), "uuid" | "tstamp")) {
        mark(items, field, Disposition::Omitted, "Source edge identity retained in report only; native outline has no per-edge identity.");
    }
    let a = point(n, "start", items)?;
    let b = point(n, "end", items)?;
    if n.head == "gr_line" {
        Ok(vec![(a, b)])
    } else {
        let c = Point::new(b.x, a.y);
        let d = Point::new(a.x, b.y);
        Ok(vec![(a, c), (c, b), (b, d), (d, a)])
    }
}
fn assemble_outline(edges: &[(Point, Point)]) -> Result<BoardOutline> {
    if edges.len() > crate::outline::MAX_OUTLINE_VERTICES {
        return Err(bad("outline exceeds 2048 vertices"));
    }
    let key = |p: Point| (p.x.0, p.y.0);
    let mut adjacent = BTreeMap::<_, Vec<usize>>::new();
    for (i, (a, b)) in edges.iter().enumerate() {
        if a == b {
            return Err(bad("zero-length outline edge"));
        }
        adjacent.entry(key(*a)).or_default().push(i);
        adjacent.entry(key(*b)).or_default().push(i);
    }
    if adjacent.values().any(|a| a.len() != 2) {
        return Err(bad("outline must close exactly with two edges per endpoint; repair gaps/branches"));
    }
    let mut used = vec![false; edges.len()];
    let mut rings = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let first = edges[start].0;
        let mut cursor = first;
        let mut edge = start;
        let mut points = Vec::new();
        loop {
            if used[edge] {
                return Err(bad("ambiguous outline loop"));
            }
            used[edge] = true;
            points.push(cursor);
            let (a, b) = edges[edge];
            cursor = if cursor == a { b } else { a };
            if cursor == first {
                break;
            }
            edge = *adjacent[&key(cursor)].iter().find(|i| !used[**i]).ok_or_else(|| bad("open outline loop"))?;
        }
        rings.push(Polygon::new(points));
        if rings.len() > 65 {
            return Err(bad("outline exceeds 64 cutouts"));
        }
    }
    let outer = rings
        .iter()
        .enumerate()
        .max_by_key(|(_, r)| r.signed_area_x2().abs())
        .map(|(i, _)| i)
        .ok_or_else(|| bad("missing supported board outline"))?;
    let outer = rings.remove(outer);
    BoardOutline::new(outer, rings).map_err(|e| bad(e.to_string()))
}

pub(crate) fn canonical_layer(s: &str) -> bool {
    matches!(
        s,
        "F.Cu"
            | "B.Cu"
            | "B.Adhes"
            | "F.Adhes"
            | "B.Paste"
            | "F.Paste"
            | "B.SilkS"
            | "F.SilkS"
            | "B.Mask"
            | "F.Mask"
            | "Dwgs.User"
            | "Cmts.User"
            | "Eco1.User"
            | "Eco2.User"
            | "Edge.Cuts"
            | "Margin"
            | "B.CrtYd"
            | "F.CrtYd"
            | "B.Fab"
            | "F.Fab"
    ) || s.strip_prefix("User.").is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

/// Shared independent bounded syntax; callers may impose smaller source limits.
pub(crate) fn parse_tree(source: &str) -> Result<(Node, Vec<FidelityItem>)> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(bad("source exceeds 8 MiB"));
    }
    let mut parser = Parser { source, at: if source.starts_with('\u{feff}') { 3 } else { 0 }, tokens: 0, text: 0, items: Vec::new() };
    let root = parser.list("root".into(), 1)?;
    parser.skip();
    if parser.at != parser.source.len() {
        return Err(bad("expected one S-expression root with no trailing data"));
    }
    Ok((root, parser.items))
}

pub(crate) fn validate_ids(n: &Node, ids: &mut BTreeSet<ObjectId>, items: &mut [FidelityItem]) -> Result<()> {
    if matches!(n.head.as_str(), "uuid" | "tstamp") {
        let id = atoms(n, 1)?[0].parse::<ObjectId>().map_err(|_| bad("malformed source UUID"))?;
        if id.as_uuid().is_nil() || !ids.insert(id) {
            return Err(bad("source UUIDs must be nonnil and globally unique"));
        }
    }
    let mut identity_fields = n.children.iter().filter(|c| matches!(c.head.as_str(), "uuid" | "tstamp"));
    if let Some(c) = identity_fields.next() {
        if identity_fields.next().is_some() {
            return Err(bad("ambiguous repeated uuid/tstamp identity fields"));
        }
        items[n.id].source_id = Some(atoms(c, 1)?[0].parse::<ObjectId>().map_err(|_| bad("malformed source UUID"))?);
    }
    for c in &n.children {
        validate_ids(c, ids, items)?;
    }
    Ok(())
}

/// Parse without external reads or model mutation. Every source list is classified.
pub fn parse(source: &str) -> Result<ParsedPcb> {
    let (root, mut items) = parse_tree(source)?;
    if root.head != "kicad_pcb" || !root.args.is_empty() {
        return Err(bad("expected one kicad_pcb root with no trailing data"));
    }
    let version = field(&root, "version", 1, &mut items)?[0].clone();
    mark(
        &mut items,
        child(&root, "version")?.unwrap(),
        Disposition::Omitted,
        "Source format date retained in fidelity report only; native format has its own schema version.",
    );
    if !matches!(version.as_str(), "20221018" | "20240108" | "20241229") {
        return Err(bad("unsupported format date; supported dates: 20221018, 20240108, 20241229"));
    }
    let mut ids = BTreeSet::new();
    validate_ids(&root, &mut ids, &mut items)?;
    let mut table = BTreeMap::new();
    let mut names = BTreeSet::new();
    for n in root.children.iter().filter(|n| n.head == "net") {
        let a = atoms(n, 2)?;
        let ordinal = a[0].parse::<u32>().map_err(|_| bad("invalid net table ordinal"))?;
        let s = if ordinal == 0 {
            if !a[1].is_empty() {
                return Err(bad("net zero must have empty name"));
            }
            String::new()
        } else {
            name(&a[1])?
        };
        if table.insert(ordinal, s.clone()).is_some() || !names.insert(s) {
            return Err(bad("duplicate/conflicting net ordinals or names cannot be represented"));
        }
        mark(&mut items, n, Disposition::Omitted, "Net table resolves exact object assignments; unused net registry entries are not persisted.");
    }
    let layers = child(&root, "layers")?.ok_or_else(|| bad("missing canonical layer table"))?;
    if !layers.args.is_empty() {
        return Err(bad("layer table cannot have scalar values"));
    }
    let mut declared = BTreeMap::new();
    let mut ordinals = BTreeSet::new();
    let mut layer_error = None;
    for l in &layers.children {
        let ordinal = l.head.parse::<u32>().map_err(|_| bad("layer ordinal must be an unsigned integer"))?;
        if !(2..=3).contains(&l.args.len())
            || !l.children.is_empty()
            || !ordinals.insert(ordinal)
            || declared.insert(l.args[0].clone(), ordinal).is_some()
        {
            return Err(bad("layer names and numeric ordinals must be unique with exactly name/type/optional display name"));
        }
        if !matches!(l.args[1].as_str(), "jumper" | "mixed" | "power" | "signal" | "user") {
            return Err(bad("unsupported layer type; use documented jumper/mixed/power/signal/user"));
        }
        if !canonical_layer(&l.args[0]) {
            layer_error = Some("unknown canonical layer name or unsupported inner copper stack");
        }
    }
    let front = declared.get("F.Cu").ok_or_else(|| bad("canonical F.Cu declaration required"))?;
    let back = declared.get("B.Cu").ok_or_else(|| bad("canonical B.Cu declaration required"))?;
    if front >= back {
        return Err(bad("canonical F.Cu must precede B.Cu in copper stack ordering"));
    }
    if !declared.contains_key("Edge.Cuts") {
        return Err(bad("canonical Edge.Cuts declaration required"));
    }
    subtree(
        &mut items,
        layers,
        if layer_error.is_some() { Disposition::Blocked } else { Disposition::Omitted },
        layer_error.unwrap_or("Canonical outer layers map to native surfaces; display names/stack metadata are not persisted."),
    );
    // Count before collecting native objects or running quadratic outline validation.
    fn count(n: &Node) -> usize {
        usize::from(matches!(n.head.as_str(), "footprint" | "pad" | "segment" | "via" | "gr_line" | "gr_rect"))
            + n.children.iter().map(count).sum::<usize>()
    }
    if count(&root) > MAX_OBJECTS {
        return Err(bad("physical object count exceeds 10000"));
    }
    let mut project = Project::new("Imported PCB");
    while ids.contains(&project.id) {
        project.id = ObjectId::new();
    }
    let mut edges = Vec::new();
    let mut refs = BTreeSet::new();
    mark(
        &mut items,
        &root,
        Disposition::Imported,
        "Supported standalone board geometry converted; no project/schematic or fabrication equivalence claim.",
    );
    for n in &root.children {
        let result: Result<()> = match n.head.as_str() {
            "version" | "net" | "layers" => Ok(()),
            "generator" | "generator_version" | "general" | "paper" | "title_block" | "setup" | "property" => {
                subtree(
                    &mut items,
                    n,
                    Disposition::Omitted,
                    "Project/presentation/setup metadata and design constraints are not persisted; native defaults are not source rules.",
                );
                Ok(())
            }
            "segment" => track(n, &table, &mut items).map(|t| {
                project.board.tracks.insert(t.id, t);
                mark(&mut items, n, Disposition::Imported, "Exact straight outer-layer native track.");
            }),
            "via" => via(n, &table, &mut items).map(|v| {
                project.board.vias.insert(v.id, v);
                mark(&mut items, n, Disposition::Imported, "Exact circular native through via.");
            }),
            "footprint" => footprint(n, &table, &mut items).and_then(|p| {
                if !refs.insert(p.designator.clone()) {
                    return Err(bad("duplicate component Reference"));
                }
                project.board.parts.insert(p.id, p);
                mark(&mut items, n, Disposition::Imported, "Exact front-side zero-angle native SMD component; no schematic link inferred.");
                Ok(())
            }),
            head if head.starts_with("gr_") => {
                let l = child(n, "layer")?.and_then(|l| l.args.first()).map(String::as_str);
                if l == Some("Edge.Cuts") && matches!(head, "gr_line" | "gr_rect") {
                    outline_edges(n, &mut items).and_then(|new| {
                        if edges.len() + new.len() > crate::outline::MAX_OUTLINE_VERTICES {
                            return Err(bad("outline exceeds 2048 vertices"));
                        }
                        edges.extend(new);
                        mark(&mut items, n, Disposition::Imported, "Exact outline edges represented by validated native polygon.");
                        Ok(())
                    })
                } else if l.is_some_and(|l| !l.ends_with(".Cu") && l != "Edge.Cuts")
                    && matches!(head, "gr_line" | "gr_rect" | "gr_circle" | "gr_arc" | "gr_poly" | "gr_curve" | "gr_text" | "gr_text_box")
                {
                    subtree(&mut items, n, Disposition::Omitted, "Non-copper, non-outline drawing/presentation object is not persisted.");
                    Ok(())
                } else {
                    Err(bad("unsupported copper/outline or unknown graphic geometry"))
                }
            }
            _ => Err(bad("unsupported board object; inspect its source token/path and export the documented supported subset")),
        };
        if let Err(e) = result {
            subtree(&mut items, n, Disposition::Blocked, &e.0);
        }
    }
    match assemble_outline(&edges) {
        Ok(o) => project.board.outline = Some(o),
        Err(e) => mark(&mut items, &root, Disposition::Blocked, &e.0),
    }
    if let Err(error) = crate::schematic::validate_project(&project, None) {
        mark(&mut items, &root, Disposition::Blocked, &format!("native project invariant: {error}"));
    }
    let imported = ImportedCounts {
        parts: project.board.parts.len(),
        pads: project.board.parts.values().map(|p| p.pads.as_ref().map_or(0, |p| p.0.len())).sum(),
        tracks: project.board.tracks.len(),
        vias: project.board.vias.len(),
        outline_vertices: project.board.outline.as_ref().map_or(0, |o| o.rings().map(|p| p.points.len()).sum()),
    };
    let omitted_lists = items.iter().filter(|i| i.disposition == Disposition::Omitted).count();
    let blocked_lists = items.iter().filter(|i| i.disposition == Disposition::Blocked).count();
    Ok(ParsedPcb { project, report: FidelityReport { format_version: version, imported, omitted_lists, blocked_lists, full_fidelity: false, items } })
}

#[cfg(test)]
mod ignored_pin_metadata_tests {
    use super::*;
    const SOURCE: &str = include_str!("../../../fixtures/import/original-two-layer.kicad_pcb");
    #[test]
    fn scalar_pin_metadata_never_hides_semantic_children_or_duplicate_fields() {
        for field in ["pinfunction", "pintype"] {
            let valid = SOURCE.replacen("(size 0.8 1.2)", &format!("(size 0.8 1.2) ({field} \"VIN\")"), 1);
            assert!(parse(&valid).unwrap().report.can_import());
            for injected in [format!("({field} \"VIN\" (zone (net 1)))"), format!("({field} \"VIN\") ({field} \"GND\")")] {
                let source = SOURCE.replacen("(size 0.8 1.2)", &format!("(size 0.8 1.2) {injected}"), 1);
                assert!(!parse(&source).unwrap().report.can_import());
            }
        }
    }
}
