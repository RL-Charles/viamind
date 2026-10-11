//! Bounded native schematic geometry. Electrical graph evaluation is separate.
use crate::{ObjectId, Project, Sheet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use viamind_geom::Nm;

pub const MAX_SHEETS: usize = 1000;
pub const MAX_SHEET_OBJECTS: usize = 10_000;
pub const MAX_PROJECT_OBJECTS: usize = 100_000;
pub const MAX_PINS: usize = 100_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Coordinate {
    pub x: Nm,
    pub y: Nm,
}
impl Coordinate {
    pub fn validate(self) -> Result<(), String> {
        if [self.x.0, self.y.0].iter().any(|n| n.unsigned_abs() > 10_000_000_000) {
            return Err("schematic coordinates must be within ±10000 mm".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PinType {
    Unspecified,
    Passive,
    Input,
    Output,
    Bidirectional,
    TriState,
    OpenCollector,
    OpenEmitter,
    PowerInput,
    PowerOutput,
    NoConnect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolPin {
    pub id: ObjectId,
    pub number: String,
    pub name: String,
    pub position: Coordinate,
    pub electrical: PinType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicSymbol {
    pub id: ObjectId,
    pub designator: String,
    pub value: String,
    pub footprint: Option<String>,
    pub position: Coordinate,
    pub rotation_quarters: u8,
    pub mirrored: bool,
    pub width: Nm,
    pub height: Nm,
    pub pins: Vec<SymbolPin>,
}
impl SchematicSymbol {
    /// Exact endpoint transform: local X reflection, then CCW quarter turns.
    pub fn pin_position(&self, local: Coordinate) -> Result<Coordinate, String> {
        if self.rotation_quarters > 3 {
            return Err("rotation_quarters must be within 0..3".into());
        }
        let mut x = i128::from(local.x.0);
        let mut y = i128::from(local.y.0);
        if self.mirrored {
            x = -x;
        }
        for _ in 0..self.rotation_quarters {
            (x, y) = (y, -x);
        }
        let position = Coordinate {
            x: Nm(i64::try_from(x + i128::from(self.position.x.0)).map_err(|_| "pin X overflow")?),
            y: Nm(i64::try_from(y + i128::from(self.position.y.0)).map_err(|_| "pin Y overflow")?),
        };
        position.validate()?;
        Ok(position)
    }
    pub fn validate(&self) -> Result<(), String> {
        name(&self.designator, false, 128)?;
        name(&self.value, true, 1024)?;
        if let Some(f) = &self.footprint {
            name(f, false, 128)?;
        }
        self.position.validate()?;
        if self.rotation_quarters > 3 {
            return Err("rotation_quarters must be within 0..3".into());
        }
        if [self.width.0, self.height.0].iter().any(|n| *n <= 0 || *n > 10_000_000_000) {
            return Err("symbol body dimensions must be greater than zero and at most 10000 mm".into());
        }
        if self.pins.len() > 1024 {
            return Err("at most 1024 pins per symbol".into());
        }
        let mut numbers = BTreeSet::new();
        for p in &self.pins {
            name(&p.number, false, 128)?;
            name(&p.name, true, 128)?;
            p.position.validate()?;
            if !numbers.insert(&p.number) {
                return Err("symbol pin numbers must be unique".into());
            }
            self.pin_position(p.position)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicWire {
    pub id: ObjectId,
    pub start: Coordinate,
    pub end: Coordinate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicJunction {
    pub id: ObjectId,
    pub position: Coordinate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LabelScope {
    Local,
    Global,
    Power,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicLabel {
    pub id: ObjectId,
    pub position: Coordinate,
    pub name: String,
    pub scope: LabelScope,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicContent {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "crate::symbol_package::bounded_packages")]
    pub packages: BTreeMap<ObjectId, crate::symbol_package::SchematicPackage>,
    pub symbols: BTreeMap<ObjectId, SchematicSymbol>,
    pub wires: BTreeMap<ObjectId, SchematicWire>,
    pub junctions: BTreeMap<ObjectId, SchematicJunction>,
    pub labels: BTreeMap<ObjectId, SchematicLabel>,
}
impl SchematicContent {
    pub fn object_count(&self) -> usize {
        self.packages
            .len()
            .saturating_add(self.symbols.len())
            .saturating_add(self.wires.len())
            .saturating_add(self.junctions.len())
            .saturating_add(self.labels.len())
    }
    pub fn pin_count(&self) -> usize {
        self.symbols.values().map(|s| s.pins.len()).sum()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.object_count() > MAX_SHEET_OBJECTS || self.pin_count() > MAX_PINS {
            return Err("sheet exceeds 10000 objects or 100000 pins".into());
        }
        for s in self.symbols.values() {
            s.validate()?;
        }
        crate::symbol_package::validate_membership(self)?;
        for w in self.wires.values() {
            w.start.validate()?;
            w.end.validate()?;
            if w.start == w.end {
                return Err("schematic wires must have distinct endpoints".into());
            }
        }
        for j in self.junctions.values() {
            j.position.validate()?;
        }
        for l in self.labels.values() {
            l.position.validate()?;
            name(&l.name, false, 128)?;
        }
        Ok(())
    }
}

pub fn name(s: &str, empty: bool, max: usize) -> Result<(), String> {
    if (!empty && s.is_empty()) || s.trim() != s || s.chars().count() > max || s.chars().any(char::is_control) {
        return Err(format!("schematic text must be trimmed, {}at most {max} characters, without controls", if empty { "" } else { "nonempty, " }));
    }
    Ok(())
}
fn insert(ids: &mut BTreeSet<ObjectId>, id: ObjectId) -> Result<(), String> {
    if id.as_uuid().is_nil() || !ids.insert(id) {
        return Err(format!("nil or duplicate schematic/project identity {id}"));
    }
    Ok(())
}
fn insert_existing(ids: &mut BTreeSet<ObjectId>, id: ObjectId) -> Result<(), String> {
    if !ids.insert(id) {
        return Err(format!("duplicate project identity {id}"));
    }
    Ok(())
}
fn keyed<T>(ids: &mut BTreeSet<ObjectId>, map: &BTreeMap<ObjectId, T>, id: impl Fn(&T) -> ObjectId) -> Result<(), String> {
    for (key, value) in map {
        if *key != id(value) {
            return Err("schematic object map key must match ID".into());
        }
        insert(ids, *key)?;
    }
    Ok(())
}

/// Check local sheet invariants, including identities in historical snapshots.
pub fn validate_sheet(sheet: &Sheet) -> Result<(), String> {
    if sheet.content.is_none() {
        return Ok(());
    }
    let mut ids = BTreeSet::new();
    insert(&mut ids, sheet.id)?;
    if let Some(content) = &sheet.content {
        name(&sheet.name, false, 128)?;
        content.validate()?;
        keyed(&mut ids, &content.packages, |v| v.id)?;
        keyed(&mut ids, &content.symbols, |v| v.id)?;
        keyed(&mut ids, &content.wires, |v| v.id)?;
        keyed(&mut ids, &content.junctions, |v| v.id)?;
        keyed(&mut ids, &content.labels, |v| v.id)?;
        let mut references = BTreeMap::new();
        let owners = crate::symbol_package::membership_owners(content)?;
        for s in content.symbols.values() {
            let owner = owners.get(&s.id).copied().unwrap_or(s.id);
            if references.insert(&s.designator, owner).is_some_and(|old| old != owner) {
                return Err("duplicate schematic designator".into());
            }
            for pin in &s.pins {
                insert(&mut ids, pin.id)?;
            }
        }
    }
    Ok(())
}

/// Validate all sheet geometry and global identities. A replacement is checked before commit.
pub fn validate_project(project: &Project, replacement: Option<&Sheet>) -> Result<(), String> {
    if project.schematic.sheets.iter().any(|(key, sheet)| *key != sheet.id) {
        return Err("schematic sheet map key must match ID".into());
    }
    let replacing = replacement.map(|s| s.id);
    let mut sheets: Vec<_> = project.schematic.sheets.values().filter(|s| Some(s.id) != replacing).collect();
    if let Some(sheet) = replacement {
        sheets.push(sheet);
    }
    if sheets.len() > MAX_SHEETS {
        return Err("at most 1000 schematic sheets".into());
    }
    let mut ids = BTreeSet::new();
    insert_existing(&mut ids, project.id)?;
    for p in project.board.parts.values() {
        insert_existing(&mut ids, p.id)?;
        for pad in p.pads.iter().flat_map(|p| &p.0) {
            insert_existing(&mut ids, pad.id)?;
        }
    }
    for id in project.board.tracks.keys().chain(project.board.vias.keys()) {
        insert_existing(&mut ids, *id)?;
    }
    let mut references = BTreeMap::new();
    let mut objects = 0usize;
    let mut pins = 0usize;
    let mut symbols = BTreeSet::new();
    let mut grouped = BTreeSet::new();
    for sheet in sheets {
        if sheet.content.is_some() {
            insert(&mut ids, sheet.id)?;
        } else {
            insert_existing(&mut ids, sheet.id)?;
        }
        if let Some(content) = &sheet.content {
            name(&sheet.name, false, 128)?;
            content.validate()?;
            objects = objects.saturating_add(content.object_count());
            pins = pins.saturating_add(content.pin_count());
            if objects > MAX_PROJECT_OBJECTS || pins > MAX_PINS {
                return Err("project exceeds 100000 schematic objects or pins".into());
            }
            keyed(&mut ids, &content.packages, |v| v.id)?;
            keyed(&mut ids, &content.symbols, |v| v.id)?;
            keyed(&mut ids, &content.wires, |v| v.id)?;
            keyed(&mut ids, &content.junctions, |v| v.id)?;
            keyed(&mut ids, &content.labels, |v| v.id)?;
            let owners = crate::symbol_package::membership_owners(content)?;
            for s in content.symbols.values() {
                symbols.insert(s.id);
                let owner = owners.get(&s.id).copied().unwrap_or(s.id);
                if owner != s.id {
                    grouped.insert(s.id);
                }
                if references.insert(&s.designator, owner).is_some_and(|old| old != owner) {
                    return Err(format!("duplicate schematic reference {}", s.designator));
                }
                for p in &s.pins {
                    insert(&mut ids, p.id)?;
                }
            }
        }
    }
    let mut linked = BTreeSet::new();
    for part in project.board.parts.values() {
        if let Some(symbol) = part.schematic_symbol {
            if grouped.contains(&symbol) {
                return Err("PCB links to individual grouped units are unsupported; package aggregation is required".into());
            }
            if symbol.as_uuid().is_nil() || !linked.insert(symbol) {
                return Err(format!("nil or multiply linked schematic symbol {symbol}"));
            }
            if ids.contains(&symbol) && !symbols.contains(&symbol) {
                return Err(format!("schematic link {symbol} refers to another object kind"));
            }
        }
    }
    Ok(())
}
