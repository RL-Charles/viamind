//! Independent strict reader for the bounded fabrication subset emitted by Viamind.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use viamind_geom::{Nm, Point};

pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_COMMANDS: usize = 100_000;
pub const MAX_PRIMITIVES: usize = 10_000;
const MAX_COORD: i64 = 99_999_999_999;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FabricationLayer {
    TopCopper,
    BottomCopper,
    TopMaskOpenings,
    BottomMaskOpenings,
    Profile,
    Drill,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FabricationPrimitive {
    Circle { center: Point, diameter: Nm },
    Rectangle { center: Point, width: Nm, height: Nm, rotation_mdeg: i32 },
    Segment { start: Point, end: Point, width: Nm },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FabricationGeometry {
    pub layer: FabricationLayer,
    pub negative_file_polarity: bool,
    pub same_coordinates: Option<ObjectId>,
    pub unverified_common_origin: bool,
    pub primitives: Vec<FabricationPrimitive>,
}
use crate::ObjectId;
#[derive(Default)]
pub struct FabricationReadBudget {
    commands: usize,
    primitives: usize,
}
impl FabricationReadBudget {
    fn command(&mut self) -> Result<(), String> {
        if self.commands >= MAX_COMMANDS {
            return Err("fabrication inspection exceeds 100000 commands".into());
        }
        self.commands += 1;
        Ok(())
    }
    fn primitive(&mut self) -> Result<(), String> {
        if self.primitives >= MAX_PRIMITIVES {
            return Err("fabrication inspection exceeds 10000 primitives".into());
        }
        self.primitives += 1;
        Ok(())
    }
}
fn invalid() -> String {
    "unsupported or malformed fabrication command/state".into()
}
fn ascii(text: &str) -> Result<(), String> {
    if text.len() > MAX_FILE_BYTES || !text.bytes().all(|b| (32..=126).contains(&b) || b == b'\n' || b == b'\r' || b == b'\t') {
        return Err("fabrication file must be printable ASCII and at most 4 MiB".into());
    }
    Ok(())
}
/// Exact decimal scaling, without floating-point conversion or sub-unit rounding.
fn decimal(s: &str, places: u32, max: i64) -> Result<i64, String> {
    let (negative, s) = if let Some(s) = s.strip_prefix('-') { (true, s) } else { (false, s.strip_prefix('+').unwrap_or(s)) };
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    if fraction.len() > places as usize && fraction[places as usize..].bytes().any(|b| b != b'0') {
        return Err("fabrication decimal has unsupported sub-unit precision".into());
    }
    let whole = whole.parse::<i64>().map_err(|_| invalid())?;
    let significant = &fraction[..fraction.len().min(places as usize)];
    let fraction = if significant.is_empty() { 0 } else { significant.parse::<i64>().map_err(|_| invalid())? };
    let value = whole
        .checked_mul(10i64.pow(places))
        .and_then(|v| v.checked_add(fraction * 10i64.pow(places - significant.len() as u32)))
        .filter(|v| *v <= max)
        .ok_or_else(invalid)?;
    Ok(if negative { -value } else { value })
}
fn integer(s: &str) -> Result<i64, String> {
    let digits = s.strip_prefix('-').or_else(|| s.strip_prefix('+')).unwrap_or(s);
    if digits.is_empty() || digits.len() > 11 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    s.parse::<i64>().ok().filter(|n| n.unsigned_abs() <= MAX_COORD as u64).ok_or_else(invalid)
}
fn point(s: &str, decimals: bool) -> Result<Point, String> {
    let (x, y) = s.strip_prefix('X').and_then(|s| s.split_once('Y')).ok_or_else(invalid)?;
    let number = |s| if decimals { decimal(s, 6, MAX_COORD) } else { integer(s) };
    Ok(Point::new(Nm(number(x)?), Nm(-number(y)?)))
}
#[derive(Clone, Copy)]
enum Aperture {
    Circle(i64),
    Rectangle(i64, i64),
}
fn layer(value: &str) -> Result<FabricationLayer, String> {
    Ok(match value {
        "Copper,L1,Top" => FabricationLayer::TopCopper,
        "Copper,L2,Bot" => FabricationLayer::BottomCopper,
        "Soldermask,Top" => FabricationLayer::TopMaskOpenings,
        "Soldermask,Bot" => FabricationLayer::BottomMaskOpenings,
        "Profile,NP" => FabricationLayer::Profile,
        _ => return Err("unsupported fabrication FileFunction".into()),
    })
}
pub fn parse_gerber(text: &str, budget: &mut FabricationReadBudget) -> Result<FabricationGeometry, String> {
    ascii(text)?;
    let mut remaining = text;
    let mut attributes = BTreeSet::new();
    let (mut format, mut metric, mut dark, mut mirror, mut scale, mut linear, mut ended, mut graphics) =
        (false, false, false, false, false, false, false, false);
    let (mut file_layer, mut polarity, mut same_coordinates) = (None, None, None);
    let mut apertures = BTreeMap::new();
    let (mut selected, mut current, mut rotation) = (None, None, 0);
    let mut primitives = Vec::new();
    while !remaining.trim().is_empty() {
        if ended {
            return Err("commands after fabrication end marker".into());
        }
        remaining = remaining.trim_start();
        let parameter = remaining.starts_with('%');
        let source = if parameter { &remaining[1..] } else { remaining };
        let end = source.find('*').ok_or_else(invalid)?;
        if end > 4096 {
            return Err("fabrication command exceeds 4096 bytes".into());
        }
        let command = &source[..end];
        remaining = &source[end + 1..];
        if parameter {
            remaining = remaining.strip_prefix('%').ok_or_else(invalid)?;
        }
        budget.command()?;
        if !parameter && command.starts_with("G04 ") {
            continue;
        }
        if parameter {
            if command == "FSLAX56Y56" {
                if format || graphics {
                    return Err(invalid());
                }
                format = true;
            } else if command == "MOMM" {
                if metric || graphics {
                    return Err(invalid());
                }
                metric = true;
            } else if let Some(attr) = command.strip_prefix("TF.") {
                if graphics {
                    return Err(invalid());
                }
                let (name, value) = attr.split_once(',').ok_or_else(invalid)?;
                if !attributes.insert(name) {
                    return Err("duplicate fabrication file attribute".into());
                }
                match name {
                    "Part" if value == "Single" => {}
                    "FileFunction" => file_layer = Some(layer(value)?),
                    "FilePolarity" => {
                        polarity = Some(match value {
                            "Positive" => false,
                            "Negative" => true,
                            _ => return Err(invalid()),
                        });
                    }
                    "SameCoordinates" => same_coordinates = Some(value.parse::<ObjectId>().map_err(|_| invalid())?),
                    _ => return Err(invalid()),
                }
            } else if let Some(definition) = command.strip_prefix("ADD") {
                if graphics || !format || !metric || apertures.len() >= 256 {
                    return Err(invalid());
                }
                let split = definition.find(['C', 'R']).ok_or_else(invalid)?;
                let id = definition[..split].parse::<u32>().ok().filter(|id| *id >= 10).ok_or_else(invalid)?;
                let shape = definition.as_bytes()[split];
                let size = definition[split + 1..].strip_prefix(',').ok_or_else(invalid)?;
                let aperture = if shape == b'C' {
                    Aperture::Circle(decimal(size, 6, 10_000_000_000)?)
                } else {
                    let (w, h) = size.split_once('X').ok_or_else(invalid)?;
                    let (w, h) = (decimal(w, 6, 10_000_000_000)?, decimal(h, 6, 10_000_000_000)?);
                    if w <= 0 || h <= 0 {
                        return Err(invalid());
                    }
                    Aperture::Rectangle(w, h)
                };
                if matches!(aperture, Aperture::Circle(n) if n < 0) || apertures.insert(id, aperture).is_some() {
                    return Err(invalid());
                }
            } else if let Some(function) = command.strip_prefix("TA.AperFunction,") {
                if graphics || !matches!(function, "SMDPad,CuDef" | "ComponentPad" | "ViaPad" | "Conductor" | "Profile") {
                    return Err(invalid());
                }
            } else if command == "TD" {
                if graphics {
                    return Err(invalid());
                }
            } else if command == "LPD" {
                dark = true;
            } else if command == "LMN" {
                mirror = true;
            } else if command == "LS1" {
                scale = true;
            } else if let Some(angle) = command.strip_prefix("LR") {
                rotation = decimal(angle, 3, 360_000)?.rem_euclid(360_000) as i32;
            } else {
                return Err(invalid());
            }
            continue;
        }
        if command == "G01" {
            linear = true;
            continue;
        }
        let ready = format && metric && dark && mirror && scale && linear && file_layer.is_some() && polarity.is_some();
        if command == "M02" {
            if !ready {
                return Err(invalid());
            }
            ended = true;
        } else if let Some(id) = command.strip_prefix('D') {
            if !ready {
                return Err(invalid());
            }
            let id = id.parse::<u32>().map_err(|_| invalid())?;
            selected = Some(*apertures.get(&id).ok_or_else(invalid)?);
        } else {
            if !ready {
                return Err(invalid());
            }
            let (position, operation) = command.rsplit_once('D').ok_or_else(invalid)?;
            let at = point(position, false)?;
            let aperture = selected.ok_or_else(invalid)?;
            graphics = true;
            let primitive = match (operation, aperture) {
                ("02", _) => {
                    current = Some(at);
                    continue;
                }
                ("01", Aperture::Circle(width)) if rotation == 0 && (width > 0 || file_layer == Some(FabricationLayer::Profile)) => {
                    FabricationPrimitive::Segment { start: current.ok_or_else(invalid)?, end: at, width: Nm(width) }
                }
                ("03", Aperture::Circle(diameter)) if diameter > 0 => FabricationPrimitive::Circle { center: at, diameter: Nm(diameter) },
                ("03", Aperture::Rectangle(width, height)) => {
                    FabricationPrimitive::Rectangle { center: at, width: Nm(width), height: Nm(height), rotation_mdeg: rotation }
                }
                _ => return Err(invalid()),
            };
            budget.primitive()?;
            primitives.push(primitive);
            current = Some(at);
        }
    }
    let file_layer = file_layer.ok_or_else(invalid)?;
    let negative = polarity.ok_or_else(invalid)?;
    if !ended
        || negative != matches!(file_layer, FabricationLayer::TopMaskOpenings | FabricationLayer::BottomMaskOpenings)
        || same_coordinates.is_some_and(|id| id.as_uuid().is_nil())
    {
        return Err(invalid());
    }
    Ok(FabricationGeometry {
        layer: file_layer,
        negative_file_polarity: negative,
        same_coordinates,
        unverified_common_origin: same_coordinates.is_none(),
        primitives,
    })
}

pub fn parse_xnc(text: &str, budget: &mut FabricationReadBudget) -> Result<FabricationGeometry, String> {
    ascii(text)?;
    let (mut header, mut metric, mut body, mut drill, mut ended) = (false, false, false, false, false);
    let mut tools = BTreeMap::new();
    let mut selected = None;
    let mut primitives = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if ended || line.len() > 4096 {
            return Err(invalid());
        }
        budget.command()?;
        if line.starts_with(';') {
            continue;
        }
        if line == "M48" && !header {
            header = true;
        } else if line == "METRIC" && header && !metric && !body {
            metric = true;
        } else if line == "%" && header && metric && !body {
            body = true;
        } else if line == "G05" && body {
            drill = true;
        } else if line == "M30" && body && drill {
            ended = true;
        } else if let Some(tool) = line.strip_prefix('T') {
            if !header || !metric {
                return Err(invalid());
            }
            if let Some((id, size)) = tool.split_once('C') {
                if body {
                    return Err(invalid());
                }
                let id = id.parse::<u8>().ok().filter(|n| (1..=99).contains(n)).ok_or_else(invalid)?;
                let diameter = decimal(size, 6, 10_000_000_000)?;
                if diameter <= 0 || tools.insert(id, diameter).is_some() {
                    return Err(invalid());
                }
            } else {
                if !body || !drill {
                    return Err(invalid());
                }
                selected = Some(*tools.get(&tool.parse::<u8>().map_err(|_| invalid())?).ok_or_else(invalid)?);
            }
        } else if body && drill {
            let center = point(line, true)?;
            let diameter = selected.ok_or_else(invalid)?;
            budget.primitive()?;
            primitives.push(FabricationPrimitive::Circle { center, diameter: Nm(diameter) });
        } else {
            return Err(invalid());
        }
    }
    if !ended {
        return Err(invalid());
    }
    Ok(FabricationGeometry {
        layer: FabricationLayer::Drill,
        negative_file_polarity: false,
        same_coordinates: None,
        unverified_common_origin: true,
        primitives,
    })
}
