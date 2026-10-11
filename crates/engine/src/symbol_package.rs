//! Complete explicit package operations; no inferred annotation or PCB aggregation.
use crate::{Command, CommandError, Delta, EngineError, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{collections::BTreeMap, path::PathBuf};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, SchematicContent, SymbolPin},
    symbol_package::{self as package, PackageUnit, SchematicPackage, SymbolPackageTemplate, UnitRole},
};

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("symbol_package", message)
}
fn file_error(message: String) -> EngineError {
    EngineError::File { message }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitPose {
    pub key: String,
    pub position: Coordinate,
    pub rotation_quarters: u8,
    pub mirrored: bool,
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use crate::{OpenProject, Origin, SaveProject};
    fn definition() -> SymbolPackageTemplate {
        package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json")).unwrap()
    }
    #[test]
    fn package_publication_protects_latest_peer_saved_and_opened_native_paths() {
        for opening in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("native.json");
            let e = EngineHandle::new(Project::new("Keep"));
            let peer = e.clone();
            let mut after = None;
            let result = e
                .save_package_before_publish(SaveSymbolPackage { template: definition(), path: path.clone() }, || {
                    if opening {
                        viamind_doc::file::save_new(&Project::new("Peer opened"), &path).unwrap();
                        peer.open_project(
                            OpenProject { path: path.clone(), discard_dirty: true, expected_cursor: Some(peer.event_cursor()) },
                            Origin::agent("peer"),
                        )
                        .unwrap();
                    } else {
                        peer.save_project(SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
                    }
                    after = Some((peer.session_state(), peer.document_status(), std::fs::read(&path).unwrap()));
                })
                .unwrap_err();
            assert!(result.to_string().contains("active native"), "{result}");
            let (session, status, bytes) = after.unwrap();
            assert_eq!(e.session_state(), session);
            assert_eq!(e.document_status(), status);
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "private stage removed");
        }
    }
    #[test]
    fn package_staging_keeps_peer_commands_live_and_publication_is_new_only() {
        for race in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("package.json");
            let e = EngineHandle::new(Project::new("Live"));
            let template = definition();
            let expected = package::to_vec(&template).unwrap();
            let mut after = None;
            let result = e.save_package_before_publish(SaveSymbolPackage { template: template.clone(), path: path.clone() }, || {
                let peer = e.clone();
                // Joining another thread would deadlock if staging retained the engine lock.
                std::thread::spawn(move || {
                    peer.execute(
                        "add_part",
                        serde_json::json!({"designator":"KEEP1","value":"peer","footprint":"F","x_mm":1,"y_mm":2}),
                        Origin::agent("peer"),
                    )
                })
                .join()
                .unwrap()
                .unwrap();
                if race {
                    std::fs::write(&path, b"racing destination").unwrap();
                }
                after = Some((e.session_state(), e.document_status()));
            });
            let (session, status) = after.unwrap();
            assert_eq!(e.session_state(), session);
            assert_eq!(e.document_status(), status);
            if race {
                assert!(result.is_err());
                assert_eq!(std::fs::read(&path).unwrap(), b"racing destination");
            } else {
                let saved = result.unwrap();
                assert_eq!((saved.unit_count, saved.pin_count), (5, 14));
                assert_eq!(std::fs::read(&path).unwrap(), expected);
                assert_eq!(package::load(&path).unwrap(), template);
            }
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceSymbolPackage {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub expected: Sheet,
    pub template: SymbolPackageTemplate,
    pub designator: String,
    pub placements: Vec<UnitPose>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceWire {
    expected_project: ObjectId,
    source_cursor: EventCursor,
    expected: Sheet,
    template: SymbolPackageTemplate,
    designator: String,
    placements: Vec<UnitPose>,
}
impl<'de> Deserialize<'de> for PlaceSymbolPackage {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        predecode_place(&value).map_err(de::Error::custom)?;
        let w: PlaceWire = serde_json::from_value(value).map_err(de::Error::custom)?;
        Ok(Self {
            expected_project: w.expected_project,
            source_cursor: w.source_cursor,
            expected: w.expected,
            template: w.template,
            designator: w.designator,
            placements: w.placements,
        })
    }
}
/// Bounds new rows/text before secondary typed allocation, including poison excess rows.
pub fn predecode_place(value: &serde_json::Value) -> Result<(), String> {
    let placements = value.get("placements").and_then(serde_json::Value::as_array).ok_or("placements must be an array")?;
    if !(2..=package::MAX_UNITS).contains(&placements.len()) {
        return Err("placements require 2..32 unit rows before typed decoding".into());
    }
    if value.get("designator").and_then(serde_json::Value::as_str).is_some_and(|s| s.len() > 4096) {
        return Err("package reference exceeds 4096 UTF-8 bytes".into());
    }
    for row in placements {
        if row.get("key").and_then(serde_json::Value::as_str).is_some_and(|s| s.len() > 8) {
            return Err("placement unit keys require at most8 ASCII bytes".into());
        }
    }
    package::predecode(value.get("template").ok_or("template is required")?)?;
    crate::schematic_budget::predecode_sheet(value.get("expected").ok_or("expected sheet is required")?, "package placement guard")
        .map_err(|e| e.to_string())
}
impl Command for PlaceSymbolPackage {
    const NAME: &'static str = "place_symbol_package";
    const DESCRIPTION: &'static str = "Place a complete explicit native multi-unit package on one sheet atomically. Requires exact project UUID/full source_cursor/expected sheet from snapshot_symbol_sheet, explicit unique shared designator and exactly one pose per template unit key. Detached template2..32 units/1..1024 total explicit distinct pins/256KiB text. Integer Nm, cardinal0..3 turns/local X mirror; exact transformed endpoints and doubled-Nm corners bounded ±10000mm before copies/UUIDs. Generates fresh package/symbol/pin identities; one Sheet delta/undo/redo. Selected/global copied object/text budgets apply. No partial package, inferred annotation, hidden power connection or PCB aggregation; those operations refuse grouped projects explicitly.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let sheet_bytes = crate::schematic_budget::sheet(&self.expected, "package placement")?;
        self.template.validate().map_err(invalid)?;
        if project.id.as_uuid().is_nil()
            || project.id != self.expected_project
            || project.schematic.sheets.get(&self.expected.id) != Some(&self.expected)
        {
            return Err(CommandError::conflict("project or target sheet changed; refresh the complete package proposal"));
        }
        viamind_doc::schematic::name(&self.designator, false, 128).map_err(invalid)?;
        if self.placements.len() != self.template.units.len() || self.placements.len() > package::MAX_UNITS {
            return Err(invalid("provide exactly one placement for each declared unit key"));
        }
        let mut poses = BTreeMap::new();
        for pose in &self.placements {
            package::unit_key(&pose.key).map_err(invalid)?;
            if poses.insert(pose.key.as_str(), pose).is_some() {
                return Err(invalid("duplicate unit placement key"));
            }
        }
        let pins = self.template.units.iter().map(|u| u.pins.len()).sum::<usize>();
        let units = self.template.units.len();
        let added = 1 + 2 * units + pins;
        let project_bytes = crate::schematic_budget::project(project, added, "global package validation")?;
        viamind_doc::schematic::validate_project(project, None).map_err(invalid)?;
        let old = self.expected.content.as_ref();
        let old_members = old
            .map(|c| package::membership_budget(c).map(|(records, _)| records.saturating_sub(c.packages.len())))
            .transpose()
            .map_err(invalid)?
            .unwrap_or(0);
        if old.map_or(0, SchematicContent::object_count).saturating_add(old_members).saturating_add(1 + 2 * units) > 10000
            || old.map_or(0, SchematicContent::pin_count).saturating_add(pins) > 10000
        {
            return Err(invalid("result exceeds10000 selected-sheet objects/membership records or pins"));
        }
        let mut added_text = self.template.name.len();
        for unit in &self.template.units {
            let pose = poses.get(unit.key.as_str()).ok_or_else(|| invalid("missing placement for a declared unit key"))?;
            SymbolPackageTemplate::validate_unit_pose(unit, pose.position, pose.rotation_quarters, pose.mirrored).map_err(invalid)?;
            added_text = added_text
                .saturating_add(unit.key.len())
                .saturating_add(unit.name.len())
                .saturating_add(self.designator.len())
                .saturating_add(self.template.value.len())
                .saturating_add(self.template.footprint.as_ref().map_or(0, String::len));
            for pin in &unit.pins {
                added_text = added_text.saturating_add(pin.number.len()).saturating_add(pin.name.len());
            }
        }
        if sheet_bytes.saturating_add(added_text) > 2 * 1024 * 1024 || project_bytes.saturating_add(added_text) > 4 * 1024 * 1024 {
            return Err(invalid("result exceeds2MiB selected-sheet or4MiB global schematic text"));
        }
        if project
            .schematic
            .sheets
            .values()
            .filter_map(|s| s.content.as_ref())
            .flat_map(|c| c.symbols.values())
            .any(|s| s.designator == self.designator)
        {
            return Err(invalid("shared package reference already exists in the project"));
        }
        // All borrowed shape/text/result checks precede new UUIDs and geometry ownership.
        let id = ObjectId::new();
        let mut members = Vec::with_capacity(units);
        let mut sheet = self.expected;
        let content = sheet.content.get_or_insert_with(SchematicContent::default);
        for unit in &self.template.units {
            let pose = poses[unit.key.as_str()];
            let symbol = self
                .template
                .unit_template(unit)
                .instantiate(&self.designator, pose.position, pose.rotation_quarters, pose.mirrored)
                .map_err(invalid)?;
            members.push(PackageUnit { key: unit.key.clone(), name: unit.name.clone(), role: unit.role, symbol_id: symbol.id });
            content.symbols.insert(symbol.id, symbol);
        }
        content.packages.insert(id, SchematicPackage { id, template_name: self.template.name, units: members });
        viamind_doc::schematic::validate_project(project, Some(&sheet)).map_err(invalid)?;
        Ok(vec![Delta::record_sheet(project, sheet.id, Some(sheet))])
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteSymbolPackage {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub expected: Sheet,
    pub package_id: ObjectId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteWire {
    expected_project: ObjectId,
    source_cursor: EventCursor,
    expected: Sheet,
    package_id: ObjectId,
}
impl<'de> Deserialize<'de> for DeleteSymbolPackage {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        crate::schematic_budget::predecode_sheet(
            value.get("expected").ok_or_else(|| de::Error::custom("expected sheet is required"))?,
            "package deletion guard",
        )
        .map_err(de::Error::custom)?;
        let wire: DeleteWire = serde_json::from_value(value).map_err(de::Error::custom)?;
        Ok(Self { expected_project: wire.expected_project, source_cursor: wire.source_cursor, expected: wire.expected, package_id: wire.package_id })
    }
}
impl Command for DeleteSymbolPackage {
    const NAME: &'static str = "delete_symbol_package";
    const DESCRIPTION: &'static str = "Delete one complete explicit package and all member symbols on its sheet in one Sheet delta/undo. Requires exact project UUID/full source_cursor/expected sheet and package_id. Preserves unrelated symbols/wires/labels/board and all wire endpoints (formerly attached wires remain explicit open wires). Bounds copied membership and global validation first. No partial unit deletion, inferred rewiring or PCB aggregation.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        crate::schematic_budget::sheet(&self.expected, "package deletion")?;
        crate::schematic_budget::project(project, 0, "global package deletion")?;
        if project.id.as_uuid().is_nil()
            || project.id != self.expected_project
            || project.schematic.sheets.get(&self.expected.id) != Some(&self.expected)
        {
            return Err(CommandError::conflict("project or sheet changed; refresh before deleting the complete package"));
        }
        viamind_doc::schematic::validate_project(project, None).map_err(invalid)?;
        let mut sheet = self.expected;
        let content = sheet.content.as_mut().ok_or_else(|| CommandError::not_found("package", self.package_id))?;
        let group = content.packages.remove(&self.package_id).ok_or_else(|| CommandError::not_found("package", self.package_id))?;
        for unit in group.units {
            content.symbols.remove(&unit.symbol_id);
        }
        viamind_doc::schematic::validate_project(project, Some(&sheet)).map_err(invalid)?;
        Ok(vec![Delta::record_sheet(project, sheet.id, Some(sheet))])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadSymbolPackage {
    pub path: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveSymbolPackage {
    pub template: SymbolPackageTemplate,
    pub path: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolPackageSaved {
    pub path: PathBuf,
    pub name: String,
    pub unit_count: usize,
    pub pin_count: usize,
}
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SymbolPackageQuery {
    pub sheet_id: Option<ObjectId>,
    pub offset: usize,
    pub limit: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackagePin {
    pub pin: SymbolPin,
    pub position: Coordinate,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageUnitRow {
    pub key: String,
    pub name: String,
    pub role: UnitRole,
    pub symbol_id: ObjectId,
    pub position: Coordinate,
    pub rotation_quarters: u8,
    pub mirrored: bool,
    pub pins: Vec<PackagePin>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolPackageRow {
    pub sheet_id: ObjectId,
    pub package_id: ObjectId,
    pub template_name: String,
    pub designator: String,
    pub value: String,
    pub footprint: Option<String>,
    pub units: Vec<PackageUnitRow>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolPackageResult {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub packages: Vec<SymbolPackageRow>,
}
impl EngineHandle {
    pub fn load_symbol_package(&self, input: LoadSymbolPackage) -> Result<SymbolPackageTemplate, EngineError> {
        package::load(&input.path).map_err(file_error)
    }
    pub fn save_symbol_package(&self, input: SaveSymbolPackage) -> Result<SymbolPackageSaved, EngineError> {
        self.save_package_before_publish(input, || {})
    }
    fn save_package_before_publish(&self, input: SaveSymbolPackage, before_publish: impl FnOnce()) -> Result<SymbolPackageSaved, EngineError> {
        let staged = package::stage_new(&input.template, &input.path).map_err(file_error)?;
        before_publish();
        let path = self.with_native_file_guard(|native| staged.publish(native)).map_err(file_error)?;
        Ok(SymbolPackageSaved {
            path,
            name: input.template.name,
            unit_count: input.template.units.len(),
            pin_count: input.template.units.iter().map(|u| u.pins.len()).sum(),
        })
    }
    /// Reuse the bounded schematic-only capture; calculate rows outside the engine lock.
    pub fn query_symbol_packages(&self, input: &SymbolPackageQuery) -> Result<SymbolPackageResult, CommandError> {
        let limit = input.limit.unwrap_or(100);
        if !(1..=100).contains(&limit) {
            return Err(invalid("limit must be within1..100"));
        }
        let snapshot = self.with_engine(|e| crate::schematic_connectivity::capture(e.project(), e.event_cursor()))?;
        if input.sheet_id.is_some_and(|id| !snapshot.sheets.iter().any(|s| s.id == id)) {
            return Err(invalid("selected package query sheet does not exist"));
        }
        let mut groups: Vec<_> = snapshot
            .sheets
            .iter()
            .filter(|s| input.sheet_id.is_none_or(|id| id == s.id))
            .flat_map(|s| s.content.iter().flat_map(move |c| c.packages.values().map(move |g| (s.id, c, g))))
            .collect();
        groups.sort_by_key(|(sid, _, g)| (*sid, g.id));
        let total = groups.len();
        let end = input.offset.saturating_add(limit).min(total);
        let mut packages = vec![];
        let mut pin_count = 0usize;
        for (sheet_id, c, g) in groups.into_iter().skip(input.offset).take(limit) {
            let symbol = &c.symbols[&g.units[0].symbol_id];
            let mut units = vec![];
            for u in &g.units {
                let s = &c.symbols[&u.symbol_id];
                pin_count = pin_count.saturating_add(s.pins.len());
                if pin_count > 10000 {
                    return Err(invalid("package response exceeds10000 pins; reduce limit"));
                }
                let pins = s
                    .pins
                    .iter()
                    .map(|p| Ok(PackagePin { pin: p.clone(), position: s.pin_position(p.position).map_err(invalid)? }))
                    .collect::<Result<_, CommandError>>()?;
                units.push(PackageUnitRow {
                    key: u.key.clone(),
                    name: u.name.clone(),
                    role: u.role,
                    symbol_id: s.id,
                    position: s.position,
                    rotation_quarters: s.rotation_quarters,
                    mirrored: s.mirrored,
                    pins,
                });
            }
            packages.push(SymbolPackageRow {
                sheet_id,
                package_id: g.id,
                template_name: g.template_name.clone(),
                designator: symbol.designator.clone(),
                value: symbol.value.clone(),
                footprint: symbol.footprint.clone(),
                units,
            });
        }
        Ok(SymbolPackageResult {
            project_id: snapshot.project_id,
            source_cursor: snapshot.source_cursor,
            total,
            next_offset: (end < total).then_some(end),
            packages,
        })
    }
}
