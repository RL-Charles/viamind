//! Bounded, read-only symbol label placement shared by native and exported views.
use crate::{ViewBounds, schematic::validate_budget};
use serde::Serialize;
use std::{collections::BTreeMap, io};
use viamind_doc::{
    ObjectId, Sheet,
    schematic::{Coordinate, SchematicSymbol, SymbolPin},
};
use viamind_geom::Nm;

pub const LABEL_FONT_NM: i64 = 1_000_000;
pub const LABEL_LINE_NM: i64 = 1_600_000;
pub const LABEL_GAP_NM: i64 = 500_000;
pub const LABEL_PAD_NM: i64 = 200_000;
const ADVANCE_NM: i64 = 700_000;
const DISPLAY_LIMIT: usize = 32;
const DISPLAY_GLYPHS: usize = 32_768;
pub const LABEL_POLICY: &str = "shared upright outside-geometry labels; 32 Unicode scalars plus explicit ...; full text retained; native labels require 10 logical points and cap 32; exports retain labels at world size";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LabelRole {
    Reference,
    Value,
    Pin,
}

#[derive(Clone, Debug, Serialize)]
pub struct SymbolLabel {
    pub id: String,
    pub symbol_id: ObjectId,
    pub object_id: ObjectId,
    pub role: LabelRole,
    pub text: String,
    pub display_text: String,
    pub abbreviated: bool,
    /// Top-left glyph origin; adapters perform explicit baseline conversion.
    pub anchor: Coordinate,
    pub envelope: ViewBounds,
    /// Presentation only: never an electrical wire, snap anchor or hit object.
    pub leader: Option<[Coordinate; 2]>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn integer(n: i128) -> io::Result<i64> {
    i64::try_from(n).map_err(|_| invalid("schematic label coordinate overflow"))
}
fn add(a: i64, b: i64) -> io::Result<i64> {
    integer(i128::from(a) + i128::from(b))
}
fn point(x: i64, y: i64) -> Coordinate {
    Coordinate { x: Nm(x), y: Nm(y) }
}
pub fn label_union(a: ViewBounds, b: ViewBounds) -> ViewBounds {
    ViewBounds { min_x: a.min_x.min(b.min_x), min_y: a.min_y.min(b.min_y), max_x: a.max_x.max(b.max_x), max_y: a.max_y.max(b.max_y) }
}
fn at_point(p: Coordinate) -> ViewBounds {
    ViewBounds { min_x: p.x.0, min_y: p.y.0, max_x: p.x.0, max_y: p.y.0 }
}
fn displayed_len(n: usize) -> usize {
    n.min(DISPLAY_LIMIT) + if n > DISPLAY_LIMIT { 3 } else { 0 }
}
fn width(text: &str) -> io::Result<i64> {
    integer(
        i128::try_from(text.chars().count()).map_err(|_| invalid("label length overflow"))? * i128::from(ADVANCE_NM) + 2 * i128::from(LABEL_PAD_NM),
    )
}
fn label(symbol: ObjectId, object: ObjectId, role: LabelRole, text: String) -> SymbolLabel {
    let abbreviated = text.chars().count() > DISPLAY_LIMIT;
    let display_text = if abbreviated { text.chars().take(DISPLAY_LIMIT).collect::<String>() + "..." } else { text.clone() };
    let prefix = match role {
        LabelRole::Reference => "reference",
        LabelRole::Value => "value",
        LabelRole::Pin => "pin-text",
    };
    SymbolLabel {
        id: format!("{prefix}-{object}"),
        symbol_id: symbol,
        object_id: object,
        role,
        text,
        display_text,
        abbreviated,
        anchor: point(0, 0),
        envelope: ViewBounds { min_x: 0, min_y: 0, max_x: 0, max_y: 0 },
        leader: None,
    }
}
fn place(label: &mut SymbolLabel, x: i64, y: i64) -> io::Result<()> {
    label.envelope = ViewBounds { min_x: x, min_y: y, max_x: add(x, width(&label.display_text)?)?, max_y: add(y, LABEL_LINE_NM)? };
    label.anchor = point(add(x, LABEL_PAD_NM)?, add(y, LABEL_PAD_NM)?);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}
fn side(symbol: &SchematicSymbol, pin: &SymbolPin) -> Side {
    let (x, y) = (i128::from(pin.position.x.0), i128::from(pin.position.y.0));
    let (left, top) = (-i128::from(symbol.width.0 / 2), -i128::from(symbol.height.0 / 2));
    let (right, bottom) = (left + i128::from(symbol.width.0), top + i128::from(symbol.height.0));
    // Distance to actual edge segments, not infinite supporting lines. Ties are
    // Left,Right,Top,Bottom. This is presentation, not electrical pin direction.
    let choices = [
        (Side::Left, left, y.clamp(top, bottom)),
        (Side::Right, right, y.clamp(top, bottom)),
        (Side::Top, x.clamp(left, right), top),
        (Side::Bottom, x.clamp(left, right), bottom),
    ];
    let local = choices.into_iter().min_by_key(|&(side, a, b)| ((x - a) * (x - a) + (y - b) * (y - b), side)).expect("four faces").0;
    let (mut dx, mut dy) = match local {
        Side::Left => (-1, 0),
        Side::Right => (1, 0),
        Side::Top => (0, -1),
        Side::Bottom => (0, 1),
    };
    if symbol.mirrored {
        dx = -dx;
    }
    for _ in 0..symbol.rotation_quarters {
        (dx, dy) = (dy, -dx);
    }
    match (dx, dy) {
        (-1, 0) => Side::Left,
        (1, 0) => Side::Right,
        (0, -1) => Side::Top,
        _ => Side::Bottom,
    }
}

/// Validate borrowed structure/text first, then construct bounded owned labels.
/// Unsupported export fonts are checked by the selected output layer adapter.
pub fn schematic_label_layout(sheet: &Sheet) -> io::Result<Vec<SymbolLabel>> {
    validate_budget("", sheet)?;
    viamind_doc::schematic::validate_sheet(sheet).map_err(invalid)?;
    let Some(content) = &sheet.content else {
        return Ok(vec![]);
    };
    let units: BTreeMap<_, _> = content.packages.values().flat_map(|g| g.units.iter().map(|u| (u.symbol_id, u))).collect();
    let mut glyphs = 0usize;
    for symbol in content.symbols.values() {
        let reference = symbol.designator.chars().count() + units.get(&symbol.id).map_or(0, |u| u.key.chars().count() + u.name.chars().count() + 4);
        glyphs = glyphs.saturating_add(displayed_len(reference));
        if !symbol.value.is_empty() {
            glyphs = glyphs.saturating_add(displayed_len(symbol.value.chars().count()));
        }
        for pin in &symbol.pins {
            glyphs = glyphs.saturating_add(displayed_len(pin.number.chars().count() + pin.name.chars().count() + 1));
        }
        if glyphs > DISPLAY_GLYPHS {
            return Err(invalid("schematic label display exceeds 32768 glyphs including qualification and abbreviation suffixes"));
        }
    }
    let mut result = Vec::new();
    for symbol in content.symbols.values() {
        let x = symbol.width.0 / 2;
        let y = symbol.height.0 / 2;
        let corners = [(-x, -y), (symbol.width.0 - x, -y), (symbol.width.0 - x, symbol.height.0 - y), (-x, symbol.height.0 - y)];
        let mut protected = at_point(symbol.position);
        for (x, y) in corners {
            protected = label_union(protected, at_point(symbol.pin_position(point(x, y)).map_err(invalid)?));
        }
        let mut pins = Vec::with_capacity(symbol.pins.len());
        for pin in &symbol.pins {
            let at = symbol.pin_position(pin.position).map_err(invalid)?;
            protected = label_union(protected, at_point(at));
            pins.push((side(symbol, pin), at, label(symbol.id, pin.id, LabelRole::Pin, format!("{} {}", pin.number, pin.name))));
        }
        pins.sort_by_key(|(side, at, label)| (*side, if matches!(side, Side::Left | Side::Right) { at.y.0 } else { at.x.0 }, label.object_id));
        let center = add(protected.min_x, integer((i128::from(protected.max_x) - i128::from(protected.min_x)) / 2)?)?;
        let mut envelope = protected;
        for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
            let lane = envelope;
            let mut next = None;
            for (pin_side, at, label) in &mut pins {
                if *pin_side != side {
                    continue;
                }
                let w = width(&label.display_text)?;
                let (x, y) = match side {
                    Side::Left | Side::Right => {
                        let y = add(at.y.0, -LABEL_LINE_NM / 2)?.max(next.unwrap_or(i64::MIN));
                        next = Some(add(y, LABEL_LINE_NM + LABEL_GAP_NM)?);
                        (if side == Side::Left { add(protected.min_x, -LABEL_GAP_NM - w)? } else { add(protected.max_x, LABEL_GAP_NM)? }, y)
                    }
                    Side::Top | Side::Bottom => {
                        let x = add(at.x.0, -w / 2)?.max(next.unwrap_or(i64::MIN));
                        next = Some(add(x, w + LABEL_GAP_NM)?);
                        (x, if side == Side::Top { add(lane.min_y, -LABEL_GAP_NM - LABEL_LINE_NM)? } else { add(lane.max_y, LABEL_GAP_NM)? })
                    }
                };
                place(label, x, y)?;
                let b = label.envelope;
                let end = match side {
                    Side::Left => point(b.max_x, add(y, LABEL_LINE_NM / 2)?),
                    Side::Right => point(b.min_x, add(y, LABEL_LINE_NM / 2)?),
                    Side::Top => point(add(x, w / 2)?, b.max_y),
                    Side::Bottom => point(add(x, w / 2)?, b.min_y),
                };
                label.leader = Some([*at, end]);
                envelope = label_union(envelope, b);
            }
        }
        let text = units.get(&symbol.id).map_or_else(|| symbol.designator.clone(), |u| format!("{}:{} ({})", symbol.designator, u.key, u.name));
        let mut reference = label(symbol.id, symbol.id, LabelRole::Reference, text);
        let w = width(&reference.display_text)?;
        place(&mut reference, add(center, -w / 2)?, add(envelope.min_y, -LABEL_GAP_NM - LABEL_LINE_NM)?)?;
        result.push(reference);
        if !symbol.value.is_empty() {
            let mut value = label(symbol.id, symbol.id, LabelRole::Value, symbol.value.clone());
            let w = width(&value.display_text)?;
            place(&mut value, add(center, -w / 2)?, add(envelope.max_y, LABEL_GAP_NM)?)?;
            result.push(value);
        }
        result.extend(pins.into_iter().map(|(_, _, label)| label));
    }
    Ok(result)
}
