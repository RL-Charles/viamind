//! Bounded, clean-room ASCII DXF straight-edge board outline import.
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_geom::{Nm, Point, Polygon};

use crate::{BoardOutline, outline::MAX_OUTLINE_VERTICES};

pub const MAX_DXF_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DxfUnits {
    #[default]
    Header,
    Mm,
    Inches,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("DXF outline: {0}")]
pub struct DxfError(pub String);
fn invalid(message: impl Into<String>) -> DxfError {
    DxfError(message.into())
}
#[derive(Clone, Copy)]
struct Pair<'a> {
    code: u16,
    value: &'a str,
}
fn pairs(text: &str) -> Result<Vec<Pair<'_>>, DxfError> {
    if text.len() > MAX_DXF_BYTES {
        return Err(invalid("file exceeds 8 MiB limit"));
    }
    if text.starts_with("AutoCAD Binary DXF") || text.contains('\0') {
        return Err(invalid("binary DXF is unsupported; export ASCII DXF"));
    }
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let mut result = Vec::new();
    while let Some(code) = lines.next() {
        let value = lines.next().ok_or_else(|| invalid("truncated group-code/value pair"))?;
        let code = code.trim().parse::<u16>().ok().filter(|c| *c <= 1071).ok_or_else(|| invalid("invalid group code; export ASCII DXF"))?;
        if code != 999 {
            result.push(Pair { code, value: value.trim() });
        }
    }
    Ok(result)
}
fn is(pair: Option<&Pair<'_>>, code: u16, value: &str) -> bool {
    pair.is_some_and(|p| p.code == code && p.value == value)
}
fn sections<'a>(pairs: &'a [Pair<'a>]) -> Result<(&'a [Pair<'a>], Option<u16>), DxfError> {
    let mut cursor = 0;
    let mut entities = None;
    let mut units = None;
    let mut header_seen = false;
    while is(pairs.get(cursor), 0, "SECTION") {
        cursor += 1;
        let name = pairs.get(cursor).filter(|p| p.code == 2).ok_or_else(|| invalid("SECTION needs a group 2 name"))?.value;
        cursor += 1;
        let start = cursor;
        while cursor < pairs.len() && !is(pairs.get(cursor), 0, "ENDSEC") {
            if is(pairs.get(cursor), 0, "SECTION") || is(pairs.get(cursor), 0, "EOF") {
                return Err(invalid("missing ENDSEC"));
            }
            cursor += 1;
        }
        if cursor == pairs.len() {
            return Err(invalid("truncated section; missing ENDSEC"));
        }
        let body = &pairs[start..cursor];
        match name {
            "ENTITIES" if entities.is_none() => entities = Some(body),
            "ENTITIES" => return Err(invalid("multiple ENTITIES sections are ambiguous")),
            "HEADER" => {
                if header_seen {
                    return Err(invalid("multiple HEADER sections are ambiguous"));
                }
                header_seen = true;
                for (i, p) in body.iter().enumerate() {
                    if p.code == 9 && p.value == "$INSUNITS" {
                        if units.is_some() {
                            return Err(invalid("duplicate $INSUNITS"));
                        }
                        let p = body.get(i + 1).filter(|p| p.code == 70).ok_or_else(|| invalid("$INSUNITS needs group 70"))?;
                        units = Some(p.value.parse().map_err(|_| invalid("invalid $INSUNITS value"))?);
                    }
                }
            }
            _ => (),
        }
        cursor += 1;
    }
    if !is(pairs.get(cursor), 0, "EOF") || cursor + 1 != pairs.len() {
        return Err(invalid("expected final EOF after sections"));
    }
    Ok((entities.ok_or_else(|| invalid("missing ENTITIES section"))?, units))
}
fn scale(units: DxfUnits, header: Option<u16>) -> Result<f64, DxfError> {
    match units {
        DxfUnits::Mm => Ok(1_000_000.0),
        DxfUnits::Inches => Ok(25_400_000.0),
        DxfUnits::Header => match header {
            Some(1) => Ok(25_400_000.0),
            Some(4) => Ok(1_000_000.0),
            Some(5) => Ok(10_000_000.0),
            Some(6) => Ok(1_000_000_000.0),
            Some(9) => Ok(25_400.0),
            _ => Err(invalid("missing/unitless/unsupported $INSUNITS; choose explicit mm or inches")),
        },
    }
}
fn scalar<'a>(fields: &[Pair<'a>], code: u16) -> Result<Option<&'a str>, DxfError> {
    let mut values = fields.iter().filter(|p| p.code == code);
    let result = values.next().map(|p| p.value);
    if values.next().is_some() {
        return Err(invalid(format!("duplicate scalar group {code}")));
    }
    Ok(result)
}
fn number(value: &str) -> Result<f64, DxfError> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid(format!("invalid finite number: {}", value.chars().take(80).collect::<String>())))
}
fn flags(fields: &[Pair<'_>]) -> Result<u16, DxfError> {
    scalar(fields, 70)?.unwrap_or("0").parse::<u16>().map_err(|_| invalid("invalid entity flags"))
}
fn straight_2d(fields: &[Pair<'_>]) -> Result<(), DxfError> {
    for code in [30, 31, 38, 39, 43, 67, 210, 220, 230] {
        scalar(fields, code)?;
    }
    for p in fields {
        let required = match p.code {
            30 | 31 | 38..=43 | 210 | 220 => Some(0.0),
            230 => Some(1.0),
            67 => Some(0.0),
            _ => None,
        };
        if let Some(required) = required
            && number(p.value)? != required
        {
            return Err(invalid(format!(
                "nonzero bulge/width/elevation, 3D extrusion or paper space (group {}); export straight 2D model-space geometry",
                p.code
            )));
        }
    }
    Ok(())
}
fn coordinate(value: &str, scale: f64) -> Result<Nm, DxfError> {
    let nm = number(value)? * scale;
    if !nm.is_finite() || nm.abs() > crate::outline::MAX_OUTLINE_COORD_NM as f64 {
        return Err(invalid("coordinates must be within +/-10000 mm after unit conversion"));
    }
    Ok(Nm(nm.round() as i64))
}
fn point(fields: &[Pair<'_>], x_code: u16, scale: f64) -> Result<Point, DxfError> {
    let required = |code| scalar(fields, code)?.ok_or_else(|| invalid(format!("missing coordinate group {code}")));
    Ok(Point { x: coordinate(required(x_code)?, scale)?, y: -coordinate(required(x_code + 10)?, scale)? })
}
fn ring(mut points: Vec<Point>) -> Result<Polygon, DxfError> {
    if points.first() == points.last() && points.len() > 1 {
        points.pop();
    }
    let polygon = Polygon::new(points);
    BoardOutline::new(polygon.clone(), Vec::new()).map_err(|e| invalid(e.to_string()))?;
    Ok(polygon)
}
fn closed(fields: &[Pair<'_>]) -> Result<(), DxfError> {
    let flags = flags(fields)?;
    if flags & 1 == 0 || flags & !129 != 0 {
        return Err(invalid("polyline must be closed and straight 2D (flags 1 or 129); close/convert it in the source drawing"));
    }
    straight_2d(fields)
}
fn lwpolyline(fields: &[Pair<'_>], scale: f64) -> Result<Polygon, DxfError> {
    closed(fields)?;
    let count = scalar(fields, 90)?
        .ok_or_else(|| invalid("LWPOLYLINE needs vertex count group 90"))?
        .parse::<usize>()
        .map_err(|_| invalid("invalid vertex count"))?;
    if count > MAX_OUTLINE_VERTICES + 1 {
        return Err(invalid("at most 2048 outline vertices are supported"));
    }
    let mut points = Vec::new();
    let mut pending_x = None;
    for p in fields {
        match p.code {
            10 => {
                if pending_x.is_some() {
                    return Err(invalid("vertex missing Y group 20"));
                }
                pending_x = Some(coordinate(p.value, scale)?);
            }
            20 => {
                let x = pending_x.take().ok_or_else(|| invalid("Y group 20 without X group 10"))?;
                points.push(Point { x, y: -coordinate(p.value, scale)? });
            }
            _ => (),
        }
    }
    if pending_x.is_some() || points.len() != count {
        return Err(invalid("LWPOLYLINE vertex count/coordinates mismatch"));
    }
    ring(points)
}
fn record<'a>(fields: &'a [Pair<'a>], cursor: &mut usize) -> Result<(&'a str, &'a [Pair<'a>]), DxfError> {
    let name = fields.get(*cursor).filter(|p| p.code == 0).ok_or_else(|| invalid("entity must begin with group 0"))?.value;
    *cursor += 1;
    let start = *cursor;
    while *cursor < fields.len() && fields[*cursor].code != 0 {
        *cursor += 1;
    }
    Ok((name, &fields[start..*cursor]))
}
fn line_loops(lines: &[(Point, Point)]) -> Result<Vec<Polygon>, DxfError> {
    let key = |p: Point| (p.x.0, p.y.0);
    let mut endpoints = BTreeMap::<_, Vec<usize>>::new();
    for (i, (a, b)) in lines.iter().enumerate() {
        if a == b {
            return Err(invalid("zero-length LINE after Nm conversion"));
        }
        endpoints.entry(key(*a)).or_default().push(i);
        endpoints.entry(key(*b)).or_default().push(i);
    }
    if endpoints.values().any(|edges| edges.len() != 2) {
        return Err(invalid("LINE endpoints must join exactly two edges; repair open gaps or branching/duplicate edges in the source drawing"));
    }
    let mut used = vec![false; lines.len()];
    let mut rings = Vec::new();
    for start in 0..lines.len() {
        if used[start] {
            continue;
        }
        let first = lines[start].0;
        let mut cursor = first;
        let mut edge = start;
        let mut points = Vec::new();
        loop {
            if used[edge] {
                return Err(invalid("ambiguous LINE loop"));
            }
            used[edge] = true;
            points.push(cursor);
            let (a, b) = lines[edge];
            cursor = if a == cursor { b } else { a };
            if cursor == first {
                break;
            }
            edge = *endpoints[&key(cursor)].iter().find(|index| !used[**index]).ok_or_else(|| invalid("open LINE loop"))?;
        }
        rings.push(ring(points)?);
    }
    Ok(rings)
}
/// Import supported closed straight-edge DXF entities without mutating a project.
pub fn import_outline_dxf(text: &str, units: DxfUnits, layer: Option<&str>) -> Result<BoardOutline, DxfError> {
    if layer.is_some_and(str::is_empty) {
        return Err(invalid("layer filter must be a nonempty exact name, or omitted"));
    }
    let pairs = pairs(text)?;
    let (entities, header) = sections(&pairs)?;
    let scale = scale(units, header)?;
    let mut cursor = 0;
    let mut rings = Vec::new();
    let mut lines = Vec::new();
    let mut total = 0;
    while cursor < entities.len() {
        let (name, fields) = record(entities, &mut cursor)?;
        let entity_layer = scalar(fields, 8)?.unwrap_or("0");
        let selected = layer.is_none_or(|layer| layer == entity_layer);
        if name == "POLYLINE" {
            let mut vertices = Vec::new();
            if selected {
                closed(fields)?;
                for code in [10, 20] {
                    if number(scalar(fields, code)?.unwrap_or("0"))? != 0.0 {
                        return Err(invalid("POLYLINE header dummy X/Y coordinates must be zero"));
                    }
                }
            }
            loop {
                let (name, fields) = record(entities, &mut cursor).map_err(|_| invalid("POLYLINE missing SEQEND"))?;
                if name == "SEQEND" {
                    break;
                }
                if name != "VERTEX" {
                    return Err(invalid("POLYLINE needs VERTEX records followed by SEQEND"));
                }
                if selected {
                    straight_2d(fields)?;
                    if flags(fields)? != 0 {
                        return Err(invalid("curve/spline/3D VERTEX flags unsupported; export straight 2D polylines"));
                    }
                    vertices.push(point(fields, 10, scale)?);
                    if vertices.len() > MAX_OUTLINE_VERTICES + 1 {
                        return Err(invalid("at most 2048 outline vertices are supported"));
                    }
                }
            }
            if selected {
                let ring = ring(vertices)?;
                total += ring.points.len();
                rings.push(ring);
            }
        } else if selected {
            match name {
                "LWPOLYLINE" => {
                    let ring = lwpolyline(fields, scale)?;
                    total += ring.points.len();
                    rings.push(ring);
                }
                "LINE" => {
                    straight_2d(fields)?;
                    lines.push((point(fields, 10, scale)?, point(fields, 11, scale)?));
                    total += 1;
                }
                _ => {
                    return Err(invalid(format!(
                        "unsupported entity {name} on layer {entity_layer}; export closed straight polylines/LINE loops, or select an outline-only layer"
                    )));
                }
            }
        }
        if total > MAX_OUTLINE_VERTICES {
            return Err(invalid("at most 2048 total outline vertices are supported"));
        }
    }
    rings.extend(line_loops(&lines)?);
    let outer = rings
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.signed_area_x2().abs())
        .map(|(i, _)| i)
        .ok_or_else(|| invalid("no supported outline entities on the selected layer"))?;
    let outer = rings.remove(outer);
    BoardOutline::new(outer, rings).map_err(|e| invalid(e.to_string()))
}
