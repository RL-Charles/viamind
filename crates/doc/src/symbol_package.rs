//! Explicit same-sheet packages and detached rectangular multi-unit definitions.
use crate::{
    ObjectId,
    schematic::SchematicContent,
    symbol_library::{SymbolTemplate, TemplatePin},
};
use schemars::JsonSchema;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::Read,
    path::{Path, PathBuf},
};
use viamind_geom::Nm;

pub const MAX_UNITS: usize = 32;
pub const MAX_PINS: usize = 1024;
pub const MAX_TEXT: usize = 256 * 1024;
pub const MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnitRole {
    Functional,
    Power,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageUnit {
    #[serde(deserialize_with = "bounded_text")]
    pub key: String,
    #[serde(deserialize_with = "bounded_text")]
    pub name: String,
    pub role: UnitRole,
    pub symbol_id: ObjectId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicPackage {
    pub id: ObjectId,
    #[serde(deserialize_with = "bounded_text")]
    pub template_name: String,
    #[serde(deserialize_with = "bounded_units")]
    pub units: Vec<PackageUnit>,
}

fn bounded_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct Text;
    impl Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("package text within 4096 UTF-8 bytes")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() > 4096 {
                return Err(E::custom("package text exceeds 4096 UTF-8 bytes"));
            }
            Ok(value.to_owned())
        }
        fn visit_string<E: de::Error>(self, value: String) -> Result<String, E> {
            if value.len() > 4096 {
                return Err(E::custom("package text exceeds 4096 UTF-8 bytes"));
            }
            Ok(value)
        }
    }
    d.deserialize_string(Text)
}
fn bounded_units<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PackageUnit>, D::Error> {
    struct Units;
    impl<'de> Visitor<'de> for Units {
        type Value = Vec<PackageUnit>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 32 package members")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
            if a.size_hint().is_some_and(|n| n > MAX_UNITS) {
                return Err(de::Error::custom("at most 32 package members"));
            }
            let mut result = Vec::new();
            while result.len() < MAX_UNITS {
                let Some(unit) = a.next_element()? else { return Ok(result) };
                result.push(unit);
            }
            if a.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("at most 32 package members"));
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Units)
}
pub(crate) fn bounded_packages<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<ObjectId, SchematicPackage>, D::Error> {
    struct Packages;
    impl<'de> Visitor<'de> for Packages {
        type Value = BTreeMap<ObjectId, SchematicPackage>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 10000 package records")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
            if a.size_hint().is_some_and(|n| n > 10_000) {
                return Err(de::Error::custom("at most 10000 package records"));
            }
            let mut result = BTreeMap::new();
            while let Some(key) = a.next_key()? {
                if result.len() == 10_000 {
                    return Err(de::Error::custom("at most 10000 package records"));
                }
                if result.insert(key, a.next_value()?).is_some() {
                    return Err(de::Error::custom("duplicate package map key"));
                }
            }
            Ok(result)
        }
    }
    d.deserialize_map(Packages)
}

pub fn unit_key(key: &str) -> Result<(), String> {
    if !(1..=8).contains(&key.len()) || !key.as_bytes()[0].is_ascii_uppercase() || !key.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err("unit key must be 1..8 ASCII uppercase letters/digits beginning with a letter".into());
    }
    Ok(())
}
/// Charge each copied membership string/record before validation or snapshot ownership.
pub fn membership_budget(content: &SchematicContent) -> Result<(usize, usize), String> {
    if content.packages.len() > 10_000 {
        return Err("at most 10000 package records".into());
    }
    let (mut count, mut bytes) = (content.packages.len(), 0usize);
    for group in content.packages.values() {
        if !(2..=MAX_UNITS).contains(&group.units.len()) {
            return Err("packages require 2..32 members".into());
        }
        count = count.saturating_add(group.units.len());
        for text in std::iter::once(group.template_name.as_str()).chain(group.units.iter().flat_map(|u| [u.key.as_str(), u.name.as_str()])) {
            bytes = bytes.saturating_add(text.len());
            if text.len() > 4096 || bytes > 4 * 1024 * 1024 {
                return Err("package membership text exceeds 4096 bytes per field or 4 MiB aggregate".into());
            }
        }
        if count > 100_000 {
            return Err("package membership exceeds 100000 records".into());
        }
    }
    Ok((count, bytes))
}
impl SchematicPackage {
    pub fn validate(&self, content: &SchematicContent) -> Result<(), String> {
        if self.id.as_uuid().is_nil() || !(2..=MAX_UNITS).contains(&self.units.len()) {
            return Err("package requires nonnil identity and 2..32 units".into());
        }
        crate::schematic::name(&self.template_name, false, 128)?;
        let mut pins = 0usize;
        for unit in &self.units {
            let symbol = content.symbols.get(&unit.symbol_id).ok_or("package member symbol missing from its sheet")?;
            pins = pins.saturating_add(symbol.pins.len());
            if symbol.pins.is_empty() || pins > MAX_PINS {
                return Err("package requires at least one pin per unit and at most 1024 total pins".into());
            }
        }
        let mut keys = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut numbers = BTreeSet::new();
        let mut shared = None;
        for unit in &self.units {
            unit_key(&unit.key)?;
            crate::schematic::name(&unit.name, false, 128)?;
            if !keys.insert(&unit.key) || !ids.insert(unit.symbol_id) {
                return Err("package unit keys and symbol identities must be unique".into());
            }
            let symbol = content.symbols.get(&unit.symbol_id).ok_or("package member symbol missing from its sheet")?;
            symbol.validate()?;
            if symbol.pins.is_empty() {
                return Err("package units require at least one explicit pin".into());
            }
            let fields = (&symbol.designator, &symbol.value, &symbol.footprint);
            if shared.is_some_and(|expected| expected != fields) {
                return Err("all package units must share reference, value and footprint".into());
            }
            shared = Some(fields);
            for pin in &symbol.pins {
                if !numbers.insert(&pin.number) {
                    return Err("package pin numbers must be unique across all units".into());
                }
                if numbers.len() > MAX_PINS {
                    return Err("at most 1024 pins per package".into());
                }
            }
        }
        Ok(())
    }
}
pub fn package_for_symbol(content: &SchematicContent, id: ObjectId) -> Option<(&SchematicPackage, &PackageUnit)> {
    content.packages.values().find_map(|g| g.units.iter().find(|u| u.symbol_id == id).map(|u| (g, u)))
}
pub fn unit_label(content: &SchematicContent, id: ObjectId) -> Option<String> {
    let symbol = content.symbols.get(&id)?;
    Some(package_for_symbol(content, id).map_or_else(|| symbol.designator.clone(), |(_, u)| format!("{}:{} ({})", symbol.designator, u.key, u.name)))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitTemplate {
    pub key: String,
    pub name: String,
    pub role: UnitRole,
    pub width: Nm,
    pub height: Nm,
    pub pins: Vec<TemplatePin>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolPackageTemplate {
    pub name: String,
    pub value: String,
    pub footprint: Option<String>,
    pub units: Vec<UnitTemplate>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateWire {
    name: String,
    value: String,
    footprint: Option<String>,
    units: Vec<UnitTemplate>,
}
impl From<TemplateWire> for SymbolPackageTemplate {
    fn from(w: TemplateWire) -> Self {
        Self { name: w.name, value: w.value, footprint: w.footprint, units: w.units }
    }
}
impl<'de> Deserialize<'de> for SymbolPackageTemplate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        predecode(&value).map_err(de::Error::custom)?;
        let wire: TemplateWire = serde_json::from_value(value).map_err(de::Error::custom)?;
        let result = Self::from(wire);
        result.validate().map_err(de::Error::custom)?;
        Ok(result)
    }
}
/// Borrowed counts precede all unit/pin typed-row allocation, including malformed excess rows.
pub fn predecode(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("package template must be an object")?;
    let units = object.get("units").and_then(Value::as_array).ok_or("package units must be an array")?;
    if !(2..=MAX_UNITS).contains(&units.len()) {
        return Err("package template requires 2..32 units".into());
    }
    let mut pins = 0usize;
    for unit in units {
        let p = unit.get("pins").and_then(Value::as_array).ok_or("unit pins must be an array")?;
        pins = pins.saturating_add(p.len());
        if p.is_empty() || pins > MAX_PINS {
            return Err("package template requires 1..1024 total pins and at least one per unit".into());
        }
    }
    let mut bytes = 0usize;
    let mut charge = |value: Option<&Value>| -> Result<(), String> {
        if let Some(text) = value.and_then(Value::as_str) {
            bytes = bytes.saturating_add(text.len());
            if text.len() > 4096 || bytes > MAX_TEXT {
                return Err("package text exceeds 4096 UTF-8 bytes per field or 256 KiB aggregate".into());
            }
        }
        Ok(())
    };
    for k in ["name", "value", "footprint"] {
        charge(object.get(k))?;
    }
    for unit in units {
        for k in ["key", "name"] {
            charge(unit.get(k))?;
        }
        for pin in unit["pins"].as_array().expect("counted pins") {
            for k in ["number", "name"] {
                charge(pin.get(k))?;
            }
        }
    }
    Ok(())
}
impl SymbolPackageTemplate {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=MAX_UNITS).contains(&self.units.len()) {
            return Err("package template requires 2..32 units".into());
        }
        crate::schematic::name(&self.name, false, 128)?;
        crate::schematic::name(&self.value, true, 1024)?;
        if let Some(f) = &self.footprint {
            crate::schematic::name(f, false, 128)?;
        }
        let mut total = self.name.len().saturating_add(self.value.len()).saturating_add(self.footprint.as_ref().map_or(0, String::len));
        let mut count = 0usize;
        let mut numbers = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for unit in &self.units {
            count = count.saturating_add(unit.pins.len());
            if unit.pins.is_empty() || count > MAX_PINS {
                return Err("package template requires 1..1024 total pins and at least one per unit".into());
            }
            unit_key(&unit.key)?;
            crate::schematic::name(&unit.name, false, 128)?;
            if !keys.insert(&unit.key) {
                return Err("duplicate package unit key".into());
            }
            if [unit.width.0, unit.height.0].iter().any(|n| *n <= 0 || *n > 10_000_000_000) {
                return Err("unit dimensions must be positive and at most 10000 mm".into());
            }
            for text in [unit.key.as_str(), unit.name.as_str()].into_iter().chain(unit.pins.iter().flat_map(|p| [p.number.as_str(), p.name.as_str()]))
            {
                total = total.saturating_add(text.len());
                if text.len() > 4096 || total > MAX_TEXT {
                    return Err("package text exceeds 4096 UTF-8 bytes per field or 256 KiB aggregate".into());
                }
            }
            for pin in &unit.pins {
                crate::schematic::name(&pin.number, false, 128)?;
                crate::schematic::name(&pin.name, true, 128)?;
                pin.position.validate()?;
                if !numbers.insert(&pin.number) {
                    return Err("package pin numbers must be unique across all units".into());
                }
            }
        }
        Ok(())
    }
    pub fn unit_template(&self, unit: &UnitTemplate) -> SymbolTemplate {
        SymbolTemplate {
            name: self.name.clone(),
            value: self.value.clone(),
            footprint: self.footprint.clone(),
            width: unit.width,
            height: unit.height,
            pins: unit.pins.clone(),
        }
    }
}
pub fn from_slice(bytes: &[u8]) -> Result<SymbolPackageTemplate, String> {
    if bytes.len() > MAX_BYTES {
        return Err("symbol package file exceeds 1 MiB".into());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope<'a> {
        format: String,
        version: u32,
        #[serde(borrow)]
        package: &'a serde_json::value::RawValue,
    }
    let e: Envelope<'_> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if e.format != "viamind-symbol-package" || e.version != 1 {
        return Err("expected viamind-symbol-package version 1".into());
    }
    let value: Value = serde_json::from_str(e.package.get()).map_err(|e| e.to_string())?;
    predecode(&value)?;
    // Reparse original bytes into the derived strict wire so duplicate known keys
    // cannot disappear through the borrowed preflight's Value representation.
    let wire: TemplateWire = serde_json::from_str(e.package.get()).map_err(|e| e.to_string())?;
    let result = SymbolPackageTemplate::from(wire);
    result.validate()?;
    Ok(result)
}
pub fn to_vec(template: &SymbolPackageTemplate) -> Result<Vec<u8>, String> {
    template.validate()?;
    #[derive(Serialize)]
    struct Envelope<'a> {
        format: &'static str,
        version: u32,
        package: &'a SymbolPackageTemplate,
    }
    let mut bytes =
        serde_json::to_vec_pretty(&Envelope { format: "viamind-symbol-package", version: 1, package: template }).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_BYTES {
        return Err("symbol package file exceeds 1 MiB".into());
    }
    Ok(bytes)
}
pub fn load(path: &Path) -> Result<SymbolPackageTemplate, String> {
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err("choose package path of 1..4096 encoded bytes".into());
    }
    #[cfg(unix)]
    let file = std::fs::File::from(
        rustix::fs::open(path, rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC, rustix::fs::Mode::empty())
            .map_err(|e| e.to_string())?,
    );
    #[cfg(not(unix))]
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("package path must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    from_slice(&bytes)
}
pub fn stage_new(template: &SymbolPackageTemplate, path: &Path) -> Result<crate::symbol_library::StagedSymbolFile, String> {
    crate::symbol_library::stage_bytes_new(&to_vec(template)?, path)
}
pub fn save_new(template: &SymbolPackageTemplate, path: &Path, native: Option<&Path>) -> Result<PathBuf, String> {
    stage_new(template, path)?.publish(native)
}

/// Shared reference validation treats a package as one namespace owner, never a suffix convention.
pub fn reference_owner(content: &SchematicContent, id: ObjectId) -> ObjectId {
    package_for_symbol(content, id).map_or(id, |(g, _)| g.id)
}
pub fn unit_pose_only(before: &crate::schematic::SchematicSymbol, after: &crate::schematic::SchematicSymbol) -> bool {
    before.id == after.id
        && before.designator == after.designator
        && before.value == after.value
        && before.footprint == after.footprint
        && before.width == after.width
        && before.height == after.height
        && before.pins == after.pins
}
/// Validate membership without introducing pin geometry copies or reference inference.
pub fn validate_membership(content: &SchematicContent) -> Result<(), String> {
    membership_owners(content).map(|_| ())
}
pub fn membership_owners(content: &SchematicContent) -> Result<BTreeMap<ObjectId, ObjectId>, String> {
    membership_budget(content)?;
    let mut members = BTreeMap::new();
    for (id, group) in &content.packages {
        if *id != group.id {
            return Err("package map key must match ID".into());
        }
        group.validate(content)?;
        for unit in &group.units {
            if members.insert(unit.symbol_id, group.id).is_some() {
                return Err("symbol belongs to multiple packages".into());
            }
        }
    }
    Ok(members)
}
pub fn package_pin_map(content: &SchematicContent, group: &SchematicPackage) -> Result<BTreeMap<String, (String, ObjectId, ObjectId)>, String> {
    group.validate(content)?;
    Ok(group
        .units
        .iter()
        .flat_map(|u| content.symbols[&u.symbol_id].pins.iter().map(move |p| (p.number.clone(), (u.key.clone(), u.symbol_id, p.id))))
        .collect())
}
