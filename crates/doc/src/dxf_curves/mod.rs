//! Bounded, certified conversion of straight and circular ASCII DXF outlines to a polygon.
//!
//! Implements `docs/specs/dxf-outline-curves.md` from public Autodesk group-code and
//! Rust floating-point documentation only. Closed LWPOLYLINE/POLYLINE bulges
//! (`0 < |b| <= 1`), ARC+LINE loops and CIRCLE rings become a validated
//! [`BoardOutline`] polygon. Every emitted chord carries a conservative analytic upper
//! bound (subarc sagitta plus enclosed source/evaluation/rounding endpoint displacement)
//! that must not exceed the explicit integer tolerance. Native outlines stay polygonal.
pub(crate) mod numeric;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::{Nm, Point, Polygon};

use crate::dxf::{DxfError, DxfUnits, Pair, record, scalar, sections};
use crate::outline::{MAX_OUTLINE_COORD_NM, MAX_OUTLINE_VERTICES};
use crate::{BoardOutline, OutlineError};
use numeric::{Decimal, Interval, MAX_ANGLE_FRACTION_DIGITS, Work, half_bulge, quarter_tangent, unit_vector};

pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_LAYER_BYTES: usize = 512;
pub const MAX_PAIRS: usize = 131_072;
pub const MAX_LINE_BYTES: usize = 4096;
pub const MAX_ENTITY_RECORDS: usize = 16_384;
pub const MAX_SOURCE_SEGMENTS: usize = 2048;
pub const MAX_VERTICES: usize = MAX_OUTLINE_VERTICES;
pub const MAX_RINGS: usize = 65;
pub const MAX_DEPTH: u32 = 24;
pub const MAX_WORK_STEPS: u64 = 262_144;
pub const MAX_TOPOLOGY_PAIRS: u64 = 4_194_304;
pub const MAX_ROWS: usize = 2048;
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;
pub const MAX_REPORT_BYTES: usize = 2 * 1024 * 1024;
pub const MIN_TOLERANCE_NM: u64 = 10;
pub const MAX_TOLERANCE_NM: u64 = 1_000_000;
/// Desktop's initially proposed value; callers always send an explicit tolerance.
pub const DEFAULT_TOLERANCE_NM: u64 = 10_000;
const DOMAIN_NM: f64 = MAX_OUTLINE_COORD_NM as f64;
const MAX_IGNORED_TYPES: usize = 32;

/// Source entity family of a fidelity row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceEntityKind {
    Line,
    Arc,
    Circle,
    Lwpolyline,
    Polyline,
}
/// Geometric classification of one selected source segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Line,
    Arc,
    Circle,
    Bulge,
}
/// Straight segments keep their rounded source endpoints; curves become certified chords.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentDisposition {
    Straight,
    ConvertedChords,
}
/// One bounded row per selected source segment or circle. Bounds are integer Nm, rounded up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SegmentRow {
    /// One-based ordinal of the top-level ENTITIES record (VERTEX/SEQEND belong to their POLYLINE).
    pub entity: u32,
    pub entity_type: SourceEntityKind,
    /// Report-only source handle (group 5) when it is 1..16 ASCII hex digits; never persisted.
    pub handle: Option<String>,
    /// Zero-based polyline vertex whose outgoing segment this row describes; zero otherwise.
    pub segment: u32,
    pub kind: SegmentKind,
    pub chords: u32,
    /// Maximum half-diagonal of the source/evaluation enclosure of any chord endpoint.
    pub enclosure_nm: u64,
    /// Maximum endpoint displacement: enclosure plus emitted integer-Nm rounding.
    pub quantization_nm: u64,
    /// Maximum analytic sagitta of any ideal subarc against its ideal chord (0 for lines).
    pub sagitta_nm: u64,
    /// Maximum certified total deviation bound of this segment's emitted chords.
    pub bound_nm: u64,
    pub disposition: SegmentDisposition,
}
/// Integer native bounds of the converted polygon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BoundsNm {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}
/// Deterministic conversion report for identical bytes and options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DxfConversionReport {
    pub units: DxfUnits,
    pub header_insunits: Option<u16>,
    pub unit_override: bool,
    pub nm_per_unit: u64,
    pub layer: Option<String>,
    pub tolerance_nm: u64,
    /// Maximum accepted conservative analytic upper bound; not a measured exact error.
    pub achieved_bound_nm: u64,
    pub straight_only: bool,
    pub entity_records: u64,
    pub selected_entities: u64,
    pub ignored_entities: u64,
    pub ignored_by_type: BTreeMap<String, u64>,
    pub selected_segments: u64,
    pub curved_segments: u64,
    pub vertices: u64,
    pub cutouts: u64,
    pub bounds_nm: BoundsNm,
    pub work_steps: u64,
    pub topology_pair_checks: u64,
    pub rows: Vec<SegmentRow>,
    pub notices: Vec<String>,
}
/// Validated polygon plus its fidelity report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DxfConversion {
    pub outline: BoardOutline,
    pub report: DxfConversionReport,
}

/// Bounded diagnostic: the formatted error never exceeds 512 UTF-8 bytes.
pub(crate) fn fail(message: impl Into<String>) -> DxfError {
    let mut message = message.into();
    let limit = MAX_DIAGNOSTIC_BYTES - "DXF outline: ".len();
    if message.len() > limit {
        let mut end = limit - 3;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str("...");
    }
    DxfError(message)
}
fn excerpt(text: &str) -> String {
    text.chars().take(32).collect::<String>().escape_debug().to_string()
}

/// Checked pair scan: size, line, pair and comment admission precede retention.
fn scan(text: &str) -> Result<Vec<Pair<'_>>, DxfError> {
    if text.len() > MAX_SOURCE_BYTES {
        return Err(fail("source exceeds the 8 MiB limit"));
    }
    if text.starts_with("AutoCAD Binary DXF") || text.contains('\0') {
        return Err(fail("binary DXF is unsupported; export ASCII DXF"));
    }
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let mut result = Vec::new();
    let mut count = 0usize;
    let mut line_number = 0usize;
    while let Some(code) = lines.next() {
        line_number += 1;
        if code.len() > MAX_LINE_BYTES {
            return Err(fail(format!("line {line_number} exceeds {MAX_LINE_BYTES} bytes")));
        }
        let value = lines.next().ok_or_else(|| fail("truncated group-code/value pair"))?;
        line_number += 1;
        if value.len() > MAX_LINE_BYTES {
            return Err(fail(format!("line {line_number} exceeds {MAX_LINE_BYTES} bytes")));
        }
        count += 1;
        if count > MAX_PAIRS {
            return Err(fail(format!("more than {MAX_PAIRS} group-code/value pairs (including comments); select or export a smaller outline")));
        }
        let code = code
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|c| *c <= 1071)
            .ok_or_else(|| fail(format!("invalid group code at line {}; export ASCII DXF", line_number - 1)))?;
        if code != 999 {
            result.push(Pair { code, value: value.trim() });
        }
    }
    Ok(result)
}

/// Exact unit conversion `value * factor * 10^shift` plus the inherited f64 scale.
#[derive(Clone, Copy)]
struct Units {
    factor: u128,
    shift: i32,
    legacy: f64,
}
fn unit_scale(requested: DxfUnits, header: Option<u16>) -> Result<Units, DxfError> {
    let legacy = crate::dxf::scale(requested, header)?;
    let (factor, shift) = match legacy as u64 {
        1_000_000 => (1, 6),
        25_400_000 => (254, 5),
        10_000_000 => (1, 7),
        1_000_000_000 => (1, 9),
        25_400 => (254, 2),
        _ => return Err(fail("unsupported unit conversion")),
    };
    Ok(Units { factor, shift, legacy })
}

/// Enclosed exact source point and its independently rounded integer DXF coordinates.
#[derive(Clone, Copy, Debug)]
struct Node {
    x: Interval,
    y: Interval,
    k: (i64, i64),
}

/// Accumulated certificate maxima for one fidelity row.
#[derive(Default)]
struct Acc {
    chords: u32,
    enclosure: f64,
    quantization: f64,
    sagitta: f64,
    bound: f64,
}
fn up_nm(value: f64) -> u64 {
    value.max(0.0).ceil() as u64
}

/// One LINE or ARC edge awaiting loop assembly; interior points run start to end.
struct Edge {
    start: (i64, i64),
    end: (i64, i64),
    interior: Vec<(i64, i64)>,
    entity: u32,
    name: &'static str,
}

struct Converter {
    units: Units,
    tolerance: u64,
    work: Work,
    vertices: usize,
    segments: usize,
    rows: Vec<SegmentRow>,
    curved: u64,
}

fn n(value: Result<Interval, &'static str>) -> Result<Interval, String> {
    value.map_err(str::to_owned)
}
fn magnitude_ok(i: Interval) -> bool {
    i.magnitude() <= DOMAIN_NM
}
fn displacement(node: &Node) -> Result<f64, String> {
    let axis = |i: Interval, k: i64| -> Result<f64, String> {
        let k = Interval::exact(k as f64);
        Ok(n(Interval::exact(i.lo).sub(k))?.magnitude().max(n(Interval::exact(i.hi).sub(k))?.magnitude()))
    };
    let (ex, ey) = (axis(node.x, node.k.0)?, axis(node.y, node.k.1)?);
    Ok(n(n(n(Interval::exact(ex).square())?.add(n(Interval::exact(ey).square())?))?.sqrt())?.hi)
}
fn enclosure(node: &Node) -> Result<f64, String> {
    let (wx, wy) = (node.x.width() * 0.5, node.y.width() * 0.5);
    Ok(n(n(n(Interval::exact(wx).square())?.add(n(Interval::exact(wy).square())?))?.sqrt())?.hi)
}
/// Strict nearest-integer rounding: refuse when the enclosure meets a half-Nm boundary.
fn strict_round(i: Interval) -> Option<i64> {
    let k = i.lo.round();
    (i.lo > k - 0.5 && i.hi < k + 0.5 && k.abs() <= DOMAIN_NM).then_some(k as i64)
}
/// Interior chord vertices may round either way; the certificate covers the whole enclosure.
fn free_round(i: Interval) -> Result<i64, String> {
    let k = (i.lo * 0.5 + i.hi * 0.5).round();
    if k.abs() <= DOMAIN_NM { Ok(k as i64) } else { Err("curve vertex outside the +/-10000 mm native domain".into()) }
}
fn free_node(x: Interval, y: Interval) -> Result<Node, String> {
    Ok(Node { x, y, k: (free_round(x)?, free_round(y)?) })
}

impl Converter {
    fn step(&mut self, steps: u64) -> Result<(), String> {
        self.work.step(steps).map_err(|e| format!("{e} ({MAX_WORK_STEPS} steps); choose a larger tolerance or simplify the source"))
    }
    fn admit_segment(&mut self) -> Result<(), String> {
        self.segments += 1;
        if self.segments > MAX_SOURCE_SEGMENTS {
            return Err(format!("more than {MAX_SOURCE_SEGMENTS} selected source segments/circles; filter a layer or simplify the source"));
        }
        Ok(())
    }
    /// Vertex admission precedes each emitted point; repeated rounded vertices fail.
    fn emit(&mut self, out: &mut Vec<(i64, i64)>, point: (i64, i64)) -> Result<(), String> {
        if self.vertices >= MAX_VERTICES {
            return Err(format!(
                "conversion needs more than {MAX_VERTICES} polygon vertices; choose a larger tolerance, split/simplify, or filter a layer"
            ));
        }
        if out.last() == Some(&point) {
            return Err("consecutive emitted vertices coincide at one integer Nm point (zero-length segment or collapsed subdivision); repair the source, choose a larger tolerance or enlarge the curve".into());
        }
        self.vertices += 1;
        out.push(point);
        Ok(())
    }
    fn source_value(&self, token: &str, what: &str) -> Result<(Interval, i64), String> {
        let decimal = Decimal::parse(token).map_err(|e| format!("{what}: {e}"))?;
        let interval = decimal.scaled(self.units.factor, self.units.shift).map_err(|e| format!("{what}: {e}"))?;
        // Rounded exactly as the straight-edge importer does, so straight bytes stay identical.
        let legacy = token.parse::<f64>().map_err(|_| format!("{what}: invalid number"))? * self.units.legacy;
        if !legacy.is_finite() || legacy.abs() > DOMAIN_NM {
            return Err("coordinates must be within +/-10000 mm after unit conversion".into());
        }
        Ok((interval, legacy.round() as i64))
    }
    fn source_node(&self, x: &str, y: &str) -> Result<Node, String> {
        let (xi, xk) = self.source_value(x, "X coordinate")?;
        let (yi, yk) = self.source_value(y, "Y coordinate")?;
        Ok(Node { x: xi, y: yi, k: (xk, yk) })
    }
    fn exact_length(&self, token: &str, what: &str) -> Result<Interval, String> {
        let decimal = Decimal::parse(token).map_err(|e| format!("{what}: {e}"))?;
        if decimal.neg || decimal.is_zero() {
            return Err(format!("{what} must be positive"));
        }
        decimal.scaled(self.units.factor, self.units.shift).map_err(|e| format!("{what}: {e}"))
    }
    fn certify(&mut self, a: &Node, b: &Node, bulge: Option<Interval>, acc: &mut Acc) -> Result<bool, String> {
        let disp = displacement(a)?.max(displacement(b)?);
        let sagitta = match bulge {
            None => 0.0,
            Some(bulge) => {
                let dx = n(b.x.sub(a.x))?.magnitude();
                let dy = n(b.y.sub(a.y))?.magnitude();
                let chord = n(n(n(Interval::exact(dx).square())?.add(n(Interval::exact(dy).square())?))?.sqrt())?.hi;
                n(n(Interval::exact(bulge.magnitude()).mul(Interval::exact(chord)))?.scale(0.5))?.hi
            }
        };
        let total = n(Interval::exact(sagitta).add(Interval::exact(disp)))?.hi;
        if up_nm(total) > self.tolerance {
            return Ok(false);
        }
        acc.chords += 1;
        acc.enclosure = acc.enclosure.max(enclosure(a)?).max(enclosure(b)?);
        acc.quantization = acc.quantization.max(disp);
        acc.sagitta = acc.sagitta.max(sagitta);
        acc.bound = acc.bound.max(total);
        Ok(true)
    }
    /// Deterministic depth-first bisection in increasing parameter order.
    fn refine(&mut self, a: Node, b: Node, bulge: Interval, depth: u32, acc: &mut Acc, out: &mut Vec<(i64, i64)>) -> Result<(), String> {
        self.step(1)?;
        if self.certify(&a, &b, Some(bulge), acc)? {
            return Ok(());
        }
        if depth >= MAX_DEPTH {
            return Err(format!(
                "subdivision depth {MAX_DEPTH} exhausted before the {} Nm tolerance was certified; choose a larger tolerance or split the source arc",
                self.tolerance
            ));
        }
        let dx = n(b.x.sub(a.x))?;
        let dy = n(b.y.sub(a.y))?;
        let half = n(bulge.scale(0.5))?;
        let mx = n(n(n(a.x.add(b.x))?.scale(0.5))?.add(n(half.mul(dy))?))?;
        let my = n(n(n(a.y.add(b.y))?.scale(0.5))?.sub(n(half.mul(dx))?))?;
        let mid = free_node(mx, my)?;
        let sub = n(half_bulge(bulge))?;
        self.refine(a, mid, sub, depth + 1, acc, out)?;
        self.emit(out, mid.k)?;
        self.refine(mid, b, sub, depth + 1, acc, out)
    }
    /// Radius, center and extrema of a bulge arc must lie inside the native domain.
    fn check_bulge(&self, a: &Node, b: &Node, bulge: Interval) -> Result<(), String> {
        if a.k == b.k {
            return Err("zero-length bulge chord after Nm conversion".into());
        }
        let dx = n(b.x.sub(a.x))?;
        let dy = n(b.y.sub(a.y))?;
        let chord = n(n(n(dx.square())?.add(n(dy.square())?))?.sqrt())?;
        let b2 = n(bulge.square())?;
        let radius = n(n(chord.mul(n(Interval::ONE.add(b2))?))?.div(n(bulge.abs().scale(4.0))?));
        let ill = "ill-conditioned bulge: radius/center leaves the supported 1 Nm..10000 mm domain; straighten or split it in the source";
        let radius = radius.map_err(|_| ill.to_owned())?;
        if radius.lo < 1.0 || radius.hi > DOMAIN_NM {
            return Err(ill.into());
        }
        let offset = n(n(Interval::ONE.sub(b2))?.div(n(bulge.scale(4.0))?)).map_err(|_| ill.to_owned())?;
        let cx = n(n(n(a.x.add(b.x))?.scale(0.5))?.sub(n(offset.mul(dy))?))?;
        let cy = n(n(n(a.y.add(b.y))?.scale(0.5))?.add(n(offset.mul(dx))?))?;
        if !magnitude_ok(cx) || !magnitude_ok(cy) {
            return Err(ill.into());
        }
        let sagitta = n(n(bulge.abs().mul(chord))?.scale(0.5))?.hi;
        for i in [a.x, b.x, a.y, b.y] {
            if i.magnitude() + sagitta > DOMAIN_NM {
                return Err("bulge arc extrema leave the +/-10000 mm native domain".into());
            }
        }
        Ok(())
    }
    fn row(&mut self, entity: u32, entity_type: SourceEntityKind, handle: Option<String>, segment: u32, kind: SegmentKind, acc: Acc) {
        let curved = kind != SegmentKind::Line;
        if curved {
            self.curved += 1;
        }
        self.rows.push(SegmentRow {
            entity,
            entity_type,
            handle,
            segment,
            kind,
            chords: acc.chords,
            enclosure_nm: up_nm(acc.enclosure),
            quantization_nm: up_nm(acc.quantization),
            sagitta_nm: up_nm(acc.sagitta),
            bound_nm: up_nm(acc.bound),
            disposition: if curved { SegmentDisposition::ConvertedChords } else { SegmentDisposition::Straight },
        });
    }
}

/// Exactly two bounded angle fields as fixed-point degrees with a shared digit count.
fn angles(start: &str, end: &str) -> Result<(i128, i128, u32), String> {
    let parse = |token: &str, what: &str| -> Result<Decimal, String> {
        let d = Decimal::parse(token).map_err(|e| format!("{what}: {e}"))?;
        if d.cmp_abs_integer(360) == std::cmp::Ordering::Greater {
            return Err(format!("{what} must be within [-360, 360] degrees"));
        }
        Ok(d)
    };
    let (a, b) = (parse(start, "ARC start angle")?, parse(end, "ARC end angle")?);
    let digits = a.fraction_digits().max(b.fraction_digits());
    if digits > MAX_ANGLE_FRACTION_DIGITS {
        return Err(format!("ARC angles need at most {MAX_ANGLE_FRACTION_DIGITS} fractional digits"));
    }
    let overflow = || "ARC angle precision overflow".to_owned();
    Ok((a.fixed(digits).ok_or_else(overflow)?, b.fixed(digits).ok_or_else(overflow)?, digits))
}

struct Fields<'a> {
    fields: &'a [Pair<'a>],
}
impl<'a> Fields<'a> {
    fn one(&self, code: u16) -> Result<Option<&'a str>, String> {
        scalar(self.fields, code).map_err(|e| e.0)
    }
    fn required(&self, code: u16, what: &str) -> Result<&'a str, String> {
        self.one(code)?.ok_or_else(|| format!("missing {what} (group {code})"))
    }
    fn zero(&self, codes: &[u16]) -> Result<(), String> {
        for &code in codes {
            if let Some(value) = self.one(code)?
                && !Decimal::parse(value).map_err(|e| format!("group {code}: {e}"))?.is_zero()
            {
                return Err(format!("nonzero elevation/thickness/width or Z (group {code}); export flat zero-width 2D model-space geometry"));
            }
        }
        Ok(())
    }
    /// Model space and standard +Z extrusion only.
    fn planar(&self) -> Result<(), String> {
        self.zero(&[67, 210, 220])?;
        if let Some(z) = self.one(230)?
            && !Decimal::parse(z).map_err(|e| format!("group 230: {e}"))?.is_one()
        {
            return Err("nonstandard extrusion/OCS (group 230 must be 1); export standard 2D geometry".into());
        }
        Ok(())
    }
    fn flags(&self) -> Result<u16, String> {
        self.one(70)?.unwrap_or("0").parse::<u16>().map_err(|_| "invalid entity flags".to_owned())
    }
    fn closed(&self) -> Result<(), String> {
        let flags = self.flags()?;
        if flags & 1 == 0 || flags & !129 != 0 {
            return Err("polyline must be closed and 2D (flags 1 or 129); close/convert it in the source drawing".into());
        }
        Ok(())
    }
    fn handle(&self) -> Option<String> {
        self.one(5).ok().flatten().filter(|h| (1..=16).contains(&h.len()) && h.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_owned)
    }
}

/// Bulge token: absent or exactly zero means a line; `0 < |b| <= 1` only.
fn bulge(token: Option<&str>) -> Result<Option<Interval>, String> {
    let Some(token) = token else { return Ok(None) };
    let d = Decimal::parse(token).map_err(|e| format!("bulge: {e}"))?;
    if d.is_zero() {
        return Ok(None);
    }
    if d.cmp_abs_integer(1) == std::cmp::Ordering::Greater {
        return Err("|bulge| > 1 (more than a semicircle) is unsupported; split the arc into at most semicircular pieces in the source".into());
    }
    Ok(Some(d.scaled(1, 0).map_err(str::to_owned)?))
}

struct Polyline<'a> {
    entity: u32,
    kind: SourceEntityKind,
    handle: Option<String>,
    vertices: Vec<(&'a str, &'a str, Option<&'a str>)>,
}

fn lwpolyline<'a>(f: &Fields<'a>) -> Result<Vec<(&'a str, &'a str, Option<&'a str>)>, String> {
    f.closed()?;
    f.zero(&[38, 39, 43])?;
    f.planar()?;
    let count = f.required(90, "LWPOLYLINE vertex count")?.parse::<usize>().map_err(|_| "invalid LWPOLYLINE vertex count".to_owned())?;
    if count > MAX_SOURCE_SEGMENTS + 1 {
        return Err(format!("LWPOLYLINE declares more than {MAX_SOURCE_SEGMENTS} segments"));
    }
    let mut vertices: Vec<(&str, Option<&str>, Option<&str>)> = Vec::new();
    for p in f.fields {
        match p.code {
            10 => {
                if vertices.last().is_some_and(|v| v.1.is_none()) {
                    return Err("vertex missing Y group 20".into());
                }
                if vertices.len() >= count {
                    return Err("LWPOLYLINE vertex count/coordinates mismatch".into());
                }
                vertices.push((p.value, None, None));
            }
            20 => {
                let v = vertices.last_mut().filter(|v| v.1.is_none()).ok_or("Y group 20 without X group 10")?;
                v.1 = Some(p.value);
            }
            42 => {
                let v = vertices.last_mut().ok_or("bulge group 42 before the first vertex")?;
                if v.2.is_some() {
                    return Err(format!("duplicate bulge for vertex {}", vertices.len() - 1));
                }
                v.2 = Some(p.value);
            }
            40 | 41 if !Decimal::parse(p.value).map_err(|e| format!("width: {e}"))?.is_zero() => {
                return Err("nonzero polyline width (group 40/41); export zero-width outlines".into());
            }
            30 | 31 if !Decimal::parse(p.value).map_err(|e| format!("Z: {e}"))?.is_zero() => {
                return Err("nonzero Z coordinate; export flat 2D geometry".into());
            }
            _ => (),
        }
    }
    if vertices.len() != count || vertices.last().is_some_and(|v| v.1.is_none()) {
        return Err("LWPOLYLINE vertex count/coordinates mismatch".into());
    }
    Ok(vertices.into_iter().map(|(x, y, b)| (x, y.unwrap_or("0"), b)).collect())
}

/// Convert the supported straight and circular subset of an ASCII DXF without mutating anything.
pub fn convert_outline_dxf(text: &str, units: DxfUnits, layer: Option<&str>, tolerance_nm: u64) -> Result<DxfConversion, DxfError> {
    if !(MIN_TOLERANCE_NM..=MAX_TOLERANCE_NM).contains(&tolerance_nm) {
        return Err(fail(format!("tolerance_nm must be an integer in [{MIN_TOLERANCE_NM}, {MAX_TOLERANCE_NM}]")));
    }
    if layer.is_some_and(str::is_empty) {
        return Err(fail("layer filter must be a nonempty exact name, or omitted"));
    }
    if layer.is_some_and(|l| l.len() > MAX_LAYER_BYTES) {
        return Err(fail(format!("layer filter exceeds {MAX_LAYER_BYTES} bytes")));
    }
    let pairs = scan(text)?;
    let (entities, header) = sections(&pairs)?;
    let unit = unit_scale(units, header)?;
    let mut c = Converter {
        units: unit,
        tolerance: tolerance_nm,
        work: Work { used: 0, limit: MAX_WORK_STEPS },
        vertices: 0,
        segments: 0,
        rows: Vec::new(),
        curved: 0,
    };
    let mut rings: Vec<Vec<(i64, i64)>> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut records = 0usize;
    let mut ordinal = 0u32;
    let mut selected_entities = 0u64;
    let mut ignored = 0u64;
    let mut ignored_by_type = BTreeMap::<String, u64>::new();
    let mut cursor = 0;
    while cursor < entities.len() {
        let (name, fields) = next_record(entities, &mut cursor, &mut records)?;
        ordinal += 1;
        let context = |e: String| fail(format!("entity {ordinal} {}: {e}", excerpt(name)));
        let f = Fields { fields };
        let entity_layer = f.one(8).map_err(context)?.unwrap_or("0");
        if entity_layer.len() > MAX_LAYER_BYTES {
            return Err(context(format!("layer name exceeds {MAX_LAYER_BYTES} bytes")));
        }
        let selected = layer.is_none_or(|l| l == entity_layer);
        if name == "POLYLINE" {
            let mut vertices = Vec::new();
            if selected {
                (|| {
                    f.closed()?;
                    f.zero(&[10, 20, 30, 39, 40, 41, 71, 72, 73, 74, 75])?;
                    f.planar()
                })()
                .map_err(context)?;
            }
            loop {
                let (child, child_fields) = next_record(entities, &mut cursor, &mut records)
                    .map_err(|e| if records > MAX_ENTITY_RECORDS { e } else { context("POLYLINE missing SEQEND".into()) })?;
                if child == "SEQEND" {
                    break;
                }
                if child != "VERTEX" {
                    return Err(context("POLYLINE needs VERTEX records followed by SEQEND".into()));
                }
                if selected {
                    let v = Fields { fields: child_fields };
                    (|| {
                        if v.flags()? != 0 {
                            return Err("curve-fit/spline/3D/mesh VERTEX flags are unsupported; export simple 2D polylines".to_owned());
                        }
                        v.zero(&[30, 40, 41, 71, 72, 73, 74])?;
                        if v.one(50)?.is_some() {
                            return Err("curve-fit tangent VERTEX (group 50) is unsupported".into());
                        }
                        if vertices.len() > MAX_SOURCE_SEGMENTS {
                            return Err(format!("POLYLINE has more than {MAX_SOURCE_SEGMENTS} segments"));
                        }
                        vertices.push((v.required(10, "vertex X")?, v.required(20, "vertex Y")?, v.one(42)?));
                        Ok(())
                    })()
                    .map_err(|e| context(format!("vertex {}: {e}", vertices.len())))?;
                }
            }
            if selected {
                selected_entities += 1;
                let ring = polyline(&mut c, Polyline { entity: ordinal, kind: SourceEntityKind::Polyline, handle: f.handle(), vertices })
                    .map_err(context)?;
                rings.push(ring);
            } else {
                note_ignored(&mut ignored, &mut ignored_by_type, name);
            }
            continue;
        }
        if !selected {
            note_ignored(&mut ignored, &mut ignored_by_type, name);
            continue;
        }
        selected_entities += 1;
        match name {
            "LWPOLYLINE" => {
                let vertices = lwpolyline(&f).map_err(context)?;
                let ring = polyline(&mut c, Polyline { entity: ordinal, kind: SourceEntityKind::Lwpolyline, handle: f.handle(), vertices })
                    .map_err(context)?;
                rings.push(ring);
            }
            "LINE" => edges.push(line(&mut c, &f, ordinal).map_err(context)?),
            "ARC" => edges.push(arc(&mut c, &f, ordinal).map_err(context)?),
            "CIRCLE" => rings.push(circle(&mut c, &f, ordinal).map_err(context)?),
            _ => {
                return Err(fail(format!(
                    "unsupported selected entity {} on layer {} (entity {ordinal}); supported: closed LWPOLYLINE/POLYLINE with |bulge|<=1, LINE, ARC, CIRCLE. Filter an outline-only layer or convert splines/ellipses/blocks in the source",
                    excerpt(name),
                    excerpt(entity_layer)
                )));
            }
        }
        if rings.len() > MAX_RINGS {
            return Err(fail(format!("more than {MAX_RINGS} rings (1 outer + 64 cutouts)")));
        }
    }
    rings.extend(edge_loops(edges)?);
    if rings.len() > MAX_RINGS {
        return Err(fail(format!("more than {MAX_RINGS} rings (1 outer + 64 cutouts)")));
    }
    let topology = topology_pairs(&rings);
    if topology > MAX_TOPOLOGY_PAIRS {
        return Err(fail(format!("polygon topology needs {topology} edge-pair checks, above {MAX_TOPOLOGY_PAIRS}")));
    }
    let mut polygons = Vec::with_capacity(rings.len());
    for (index, ring) in rings.into_iter().enumerate() {
        let polygon = Polygon::new(ring.into_iter().map(|(x, y)| Point { x: Nm(x), y: -Nm(y) }).collect());
        BoardOutline::new(polygon.clone(), Vec::new()).map_err(|e: OutlineError| fail(format!("ring {index}: {e}")))?;
        polygons.push(polygon);
    }
    let outer = polygons
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.signed_area_x2().abs())
        .map(|(i, _)| i)
        .ok_or_else(|| fail("no supported outline entities on the selected layer"))?;
    let outer = polygons.remove(outer);
    let outline = BoardOutline::new(outer, polygons).map_err(|e| fail(e.to_string()))?;
    let bounds = outline.rings().flat_map(|r| r.points.iter()).fold(
        BoundsNm { min_x: i64::MAX, min_y: i64::MAX, max_x: i64::MIN, max_y: i64::MIN },
        |b, p| BoundsNm { min_x: b.min_x.min(p.x.0), min_y: b.min_y.min(p.y.0), max_x: b.max_x.max(p.x.0), max_y: b.max_y.max(p.y.0) },
    );
    let straight_only = c.curved == 0;
    let achieved = c.rows.iter().map(|r| r.bound_nm).max().unwrap_or(0);
    let report = DxfConversionReport {
        units,
        header_insunits: header,
        unit_override: units != DxfUnits::Header,
        nm_per_unit: unit.legacy as u64,
        layer: layer.map(str::to_owned),
        tolerance_nm,
        achieved_bound_nm: achieved,
        straight_only,
        entity_records: records as u64,
        selected_entities,
        ignored_entities: ignored,
        ignored_by_type,
        selected_segments: c.segments as u64,
        curved_segments: c.curved,
        vertices: outline.rings().map(|r| r.points.len() as u64).sum(),
        cutouts: outline.cutouts.len() as u64,
        bounds_nm: bounds,
        work_steps: c.work.used,
        topology_pair_checks: topology,
        rows: c.rows,
        notices: notices(layer, straight_only, ignored),
    };
    Ok(DxfConversion { outline, report })
}
fn next_record<'a>(entities: &'a [Pair<'a>], cursor: &mut usize, records: &mut usize) -> Result<(&'a str, &'a [Pair<'a>]), DxfError> {
    *records += 1;
    if *records > MAX_ENTITY_RECORDS {
        return Err(fail(format!("more than {MAX_ENTITY_RECORDS} entity records (including unselected, VERTEX and SEQEND)")));
    }
    record(entities, cursor)
}
fn note_ignored(ignored: &mut u64, by_type: &mut BTreeMap<String, u64>, name: &str) {
    *ignored += 1;
    let key = if name.len() <= 32 && (by_type.contains_key(name) || by_type.len() < MAX_IGNORED_TYPES - 1) { name } else { "(other)" };
    *by_type.entry(key.to_owned()).or_default() += 1;
}
fn notices(layer: Option<&str>, straight_only: bool, ignored: u64) -> Vec<String> {
    let mut notices = vec![
        "Native BoardOutline keeps only this validated polygon; analytic arcs and circles are not stored, and later DRC, fabrication and editing use the polygon.".to_owned(),
        "Each chord bound is a conservative analytic upper bound (subarc sagitta plus enclosed source/evaluation error and integer-Nm endpoint rounding), not a measured exact error.".to_owned(),
        "Source endpoints are rounded independently to integer Nm as in the straight-edge importer; endpoints join only when those integers are equal. No welding, snapping or shape repair.".to_owned(),
        "DXF X maps to native X and DXF Y is negated once; no origin shift, rotation or scale inference. Explicit unit overrides are reported.".to_owned(),
        "Conversion tolerance does not guarantee preservation of source topology, fabrication equivalence or manufacturing clearances.".to_owned(),
        "Splines, ellipses, hatches, INSERT/BLOCK expansion, 3D/OCS, widths and |bulge| > 1 are refused when selected.".to_owned(),
    ];
    notices.push(match layer {
        Some(_) => format!(
            "Exact layer filter selected entities; POLYLINE VERTEX children follow their parent's layer. {ignored} unselected entities ignored."
        ),
        None => format!("All ENTITIES layers selected; POLYLINE VERTEX children follow their parent. {ignored} entities ignored."),
    });
    if straight_only {
        notices.push("Straight-only source: zero curvature sagitta; reported bounds are inherited source-to-Nm rounding only.".to_owned());
    }
    notices
}
/// Upper bound on the edge-pair checks of per-ring and whole-outline validation.
fn topology_pairs(rings: &[Vec<(i64, i64)>]) -> u64 {
    let sizes: Vec<u64> = rings.iter().map(|r| r.len() as u64).collect();
    let total: u64 = sizes.iter().sum();
    let own: u64 = sizes.iter().map(|n| n * n.saturating_sub(1)).sum();
    let squares: u64 = sizes.iter().map(|n| n * n).sum();
    own + (total * total - squares) / 2
}

fn polyline(c: &mut Converter, mut p: Polyline<'_>) -> Result<Vec<(i64, i64)>, String> {
    let mut nodes = Vec::with_capacity(p.vertices.len());
    for (index, (x, y, b)) in p.vertices.iter().enumerate() {
        let node = c.source_node(x, y).map_err(|e| format!("vertex {index}: {e}"))?;
        let b = bulge(*b).map_err(|e| format!("vertex {index}: {e}"))?;
        nodes.push((node, b));
    }
    if nodes.len() > 1 && nodes.first().map(|n| n.0.k) == nodes.last().map(|n| n.0.k) {
        if nodes.last().is_some_and(|n| n.1.is_some()) {
            return Err("final vertex repeats the first with a nonzero closing bulge; ambiguous closing geometry".into());
        }
        nodes.pop();
        p.vertices.pop();
    }
    if nodes.len() < 2 {
        return Err("closed polyline needs at least two distinct vertices".into());
    }
    let mut out = Vec::new();
    for i in 0..nodes.len() {
        c.admit_segment()?;
        let (a, b) = (nodes[i], nodes[(i + 1) % nodes.len()]);
        let mut acc = Acc::default();
        c.emit(&mut out, a.0.k).map_err(|e| format!("vertex {i}: {e}"))?;
        let kind = match a.1 {
            None => {
                if !c.certify(&a.0, &b.0, None, &mut acc)? {
                    return Err(format!("vertex {i}: straight endpoint rounding exceeds the tolerance"));
                }
                SegmentKind::Line
            }
            Some(bulge) => {
                (|| {
                    c.check_bulge(&a.0, &b.0, bulge)?;
                    c.refine(a.0, b.0, bulge, 0, &mut acc, &mut out)
                })()
                .map_err(|e| format!("vertex {i} bulge: {e}"))?;
                SegmentKind::Bulge
            }
        };
        c.row(p.entity, p.kind, p.handle.clone(), i as u32, kind, acc);
    }
    if out.len() > 1 && out.first() == out.last() {
        return Err("closing segment collapsed to one integer Nm point".into());
    }
    Ok(out)
}

fn line(c: &mut Converter, f: &Fields<'_>, entity: u32) -> Result<Edge, String> {
    f.zero(&[30, 31, 39])?;
    f.planar()?;
    c.admit_segment()?;
    let a = c.source_node(f.required(10, "start X")?, f.required(20, "start Y")?)?;
    let b = c.source_node(f.required(11, "end X")?, f.required(21, "end Y")?)?;
    if a.k == b.k {
        return Err("zero-length LINE after Nm conversion".into());
    }
    c.vertices += 1;
    if c.vertices > MAX_VERTICES {
        return Err(format!("conversion needs more than {MAX_VERTICES} polygon vertices"));
    }
    let mut acc = Acc::default();
    if !c.certify(&a, &b, None, &mut acc)? {
        return Err("straight endpoint rounding exceeds the tolerance".into());
    }
    c.row(entity, SourceEntityKind::Line, f.handle(), 0, SegmentKind::Line, acc);
    Ok(Edge { start: a.k, end: b.k, interior: Vec::new(), entity, name: "LINE" })
}

/// Center, radius and their domain admission shared by ARC and CIRCLE.
fn circle_frame(c: &Converter, f: &Fields<'_>) -> Result<(Interval, Interval, Interval), String> {
    f.zero(&[30, 39])?;
    f.planar()?;
    let (cx, _) = c.source_value(f.required(10, "center X")?, "center X")?;
    let (cy, _) = c.source_value(f.required(20, "center Y")?, "center Y")?;
    let r = c.exact_length(f.required(40, "radius")?, "radius")?;
    if r.lo < 1.0 || r.hi > DOMAIN_NM {
        return Err("radius must be within 1 Nm..10000 mm after unit conversion".into());
    }
    if !magnitude_ok(cx) || !magnitude_ok(cy) {
        return Err("curve center outside the +/-10000 mm native domain".into());
    }
    Ok((cx, cy, r))
}
fn on_circle(cx: Interval, cy: Interval, r: Interval, (cos, sin): (Interval, Interval)) -> Result<(Interval, Interval), String> {
    let x = n(cx.add(n(r.mul(cos))?))?;
    let y = n(cy.add(n(r.mul(sin))?))?;
    if !magnitude_ok(x) || !magnitude_ok(y) {
        return Err("curve extrema leave the +/-10000 mm native domain".into());
    }
    Ok((x, y))
}

fn arc(c: &mut Converter, f: &Fields<'_>, entity: u32) -> Result<Edge, String> {
    let (cx, cy, r) = circle_frame(c, f)?;
    let (start, end, digits) = angles(f.required(50, "start angle")?, f.required(51, "end angle")?)?;
    c.admit_segment()?;
    let overflow = || "ARC angle precision overflow".to_owned();
    let quarter = 10i128.checked_pow(digits).and_then(|s| s.checked_mul(90)).ok_or_else(overflow)?;
    let full = quarter * 4;
    let a = start.rem_euclid(full);
    let sweep = (end - start).rem_euclid(full);
    if sweep == 0 {
        return Err("ARC start and end angles coincide (modulo 360); use CIRCLE for a full circle".into());
    }
    let stop = a + sweep;
    let mut breaks = vec![a];
    let mut t = (a / quarter + 1) * quarter;
    while t < stop {
        breaks.push(t);
        t += quarter;
    }
    breaks.push(stop);
    let mut nodes = Vec::with_capacity(breaks.len());
    for (index, &angle) in breaks.iter().enumerate() {
        let unit = unit_vector(angle, digits, &mut c.work).map_err(str::to_owned)?;
        let (x, y) = on_circle(cx, cy, r, unit)?;
        let node = if index == 0 || index + 1 == breaks.len() {
            let k = strict_round(x).zip(strict_round(y)).ok_or(
                "ARC endpoint enclosure meets a half-Nm rounding boundary; its integer Nm join point is unresolved (adjust the source coordinates)",
            )?;
            Node { x, y, k }
        } else {
            free_node(x, y)?
        };
        nodes.push(node);
    }
    let (first, last) = (nodes[0].k, nodes[nodes.len() - 1].k);
    if first == last {
        return Err("ARC endpoints coincide after Nm conversion; use CIRCLE or split the arc".into());
    }
    let mut acc = Acc::default();
    let mut path = vec![first];
    c.vertices += 1;
    if c.vertices > MAX_VERTICES {
        return Err(format!("conversion needs more than {MAX_VERTICES} polygon vertices"));
    }
    for i in 0..nodes.len() - 1 {
        let bulge = quarter_tangent(breaks[i + 1] - breaks[i], digits, &mut c.work).map_err(str::to_owned)?;
        if i > 0 {
            c.emit(&mut path, nodes[i].k)?;
        }
        c.refine(nodes[i], nodes[i + 1], bulge, 0, &mut acc, &mut path)?;
    }
    if path.last() == Some(&last) {
        return Err("subdivision collapsed onto the ARC end point; choose a larger tolerance or enlarge the source curve".into());
    }
    c.row(entity, SourceEntityKind::Arc, f.handle(), 0, SegmentKind::Arc, acc);
    path.remove(0);
    Ok(Edge { start: first, end: last, interior: path, entity, name: "ARC" })
}

fn circle(c: &mut Converter, f: &Fields<'_>, entity: u32) -> Result<Vec<(i64, i64)>, String> {
    let (cx, cy, r) = circle_frame(c, f)?;
    c.admit_segment()?;
    let mut nodes = Vec::with_capacity(4);
    for quadrant in 0..4i128 {
        let unit = unit_vector(quadrant * 90, 0, &mut c.work).map_err(str::to_owned)?;
        let (x, y) = on_circle(cx, cy, r, unit)?;
        nodes.push(free_node(x, y)?);
    }
    let bulge = n(n(Interval::exact(2.0).sqrt())?.sub(Interval::ONE))?;
    let mut acc = Acc::default();
    let mut out = Vec::new();
    for i in 0..4 {
        c.emit(&mut out, nodes[i].k)?;
        c.refine(nodes[i], nodes[(i + 1) % 4], bulge, 0, &mut acc, &mut out)?;
    }
    if out.first() == out.last() {
        return Err("CIRCLE subdivision collapsed; choose a larger tolerance or enlarge the source circle".into());
    }
    c.row(entity, SourceEntityKind::Circle, f.handle(), 0, SegmentKind::Circle, acc);
    Ok(out)
}

/// Join LINE/ARC edges at exactly equal rounded endpoints of degree two, in source order.
fn edge_loops(edges: Vec<Edge>) -> Result<Vec<Vec<(i64, i64)>>, DxfError> {
    let mut endpoints = BTreeMap::<(i64, i64), Vec<usize>>::new();
    for (i, e) in edges.iter().enumerate() {
        endpoints.entry(e.start).or_default().push(i);
        endpoints.entry(e.end).or_default().push(i);
    }
    if let Some((point, list)) = endpoints.iter().find(|(_, list)| list.len() != 2) {
        let e = &edges[list[0]];
        return Err(fail(format!(
            "{} endpoint ({}, {}) Nm of entity {} joins {} edges; LINE/ARC endpoints must join exactly two edges (repair open gaps, branches or duplicates in the source)",
            e.name,
            point.0,
            -point.1,
            e.entity,
            list.len()
        )));
    }
    let mut used = vec![false; edges.len()];
    let mut rings = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let first = edges[start].start;
        let mut cursor = first;
        let mut edge = start;
        let mut points = Vec::new();
        loop {
            if used[edge] {
                return Err(fail("ambiguous LINE/ARC loop"));
            }
            used[edge] = true;
            points.push(cursor);
            let e = &edges[edge];
            if e.start == cursor {
                points.extend(e.interior.iter().copied());
                cursor = e.end;
            } else {
                points.extend(e.interior.iter().rev().copied());
                cursor = e.start;
            }
            if cursor == first {
                break;
            }
            edge = *endpoints[&cursor].iter().find(|i| !used[**i]).ok_or_else(|| fail("open LINE/ARC loop"))?;
        }
        if rings.len() >= MAX_RINGS {
            return Err(fail(format!("more than {MAX_RINGS} rings (1 outer + 64 cutouts)")));
        }
        rings.push(points);
    }
    Ok(rings)
}
