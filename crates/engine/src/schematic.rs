//! Atomic sheet edits with exact project/sheet guards and stable pin identities.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, LabelScope, PinType, SchematicContent, SchematicJunction, SchematicLabel, SchematicSymbol, SchematicWire, SymbolPin},
};
use viamind_geom::Nm;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolPinInput {
    pub number: String,
    pub name: String,
    pub position: Coordinate,
    pub electrical: PinType,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolInput {
    /// Omit to create; supplied IDs must already identify a symbol on this sheet.
    pub id: Option<ObjectId>,
    pub designator: String,
    pub value: String,
    pub footprint: Option<String>,
    pub position: Coordinate,
    pub rotation_quarters: u8,
    pub mirrored: bool,
    pub width: Nm,
    pub height: Nm,
    pub pins: Vec<SymbolPinInput>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchematicEdit {
    Symbol { symbol: SymbolInput },
    Wire { id: Option<ObjectId>, start: Coordinate, end: Coordinate },
    Junction { id: Option<ObjectId>, position: Coordinate },
    Label { id: Option<ObjectId>, position: Coordinate, name: String, scope: LabelScope },
    Delete { id: ObjectId },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditSchematicSheet {
    pub expected_project: ObjectId,
    /// Omit together with expected to create a new sheet.
    pub sheet_id: Option<ObjectId>,
    pub expected: Option<Sheet>,
    pub name: String,
    pub edits: Vec<SchematicEdit>,
}
fn existing_id<T>(id: Option<ObjectId>, map: &std::collections::BTreeMap<ObjectId, T>, kind: &str) -> Result<ObjectId, CommandError> {
    match id {
        Some(id) if map.contains_key(&id) => Ok(id),
        Some(id) => Err(CommandError::not_found(kind, id)),
        None => Ok(ObjectId::new()),
    }
}
impl Command for EditSchematicSheet {
    const NAME: &'static str = "edit_schematic_sheet";
    const DESCRIPTION: &'static str = "Atomically create or edit a schematic sheet guarded by exact project and complete expected sheet snapshots. Omit sheet_id and expected together to create. Upsert symbols with numbered local pins, wires, junctions or local/global/power labels; omit object IDs for creation, existing IDs for replacement. Matching pin numbers retain identities. Integer nanometres, cardinal quarter turns, local X mirror. Maximum 1000 edits. One sheet transaction/undo. Does not compute connectivity, ERC or board annotation.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project {
            return Err(CommandError::conflict("project changed; resynchronize schematic"));
        }
        if self.edits.len() > 1000 {
            return Err(CommandError::invalid("edits", "at most 1000 edits per call"));
        }
        let mut sheet = match (self.sheet_id, &self.expected) {
            (None, None) => Sheet::new(&self.name),
            (Some(id), Some(expected)) if project.schematic.sheet(&id) == Some(expected) => expected.clone(),
            (Some(_), Some(_)) => return Err(CommandError::conflict("sheet changed or no longer exists; reload schematic")),
            _ => return Err(CommandError::invalid("expected", "supply sheet_id and exact expected sheet together, or omit both for creation")),
        };
        sheet.name = self.name;
        let content = sheet.content.get_or_insert_with(SchematicContent::default);
        for edit in self.edits {
            match edit {
                SchematicEdit::Symbol { symbol: s } => {
                    let id = existing_id(s.id, &content.symbols, "schematic symbol")?;
                    if s.pins.len() > 1024 {
                        return Err(CommandError::invalid("pins", "at most 1024 pins per symbol"));
                    }
                    let old = content.symbols.get(&id);
                    let pins = s
                        .pins
                        .into_iter()
                        .map(|p| SymbolPin {
                            id: old.and_then(|s| s.pins.iter().find(|pin| pin.number == p.number)).map(|p| p.id).unwrap_or_default(),
                            number: p.number,
                            name: p.name,
                            position: p.position,
                            electrical: p.electrical,
                        })
                        .collect();
                    let after = SchematicSymbol {
                        id,
                        designator: s.designator,
                        value: s.value,
                        footprint: s.footprint,
                        position: s.position,
                        rotation_quarters: s.rotation_quarters,
                        mirrored: s.mirrored,
                        width: s.width,
                        height: s.height,
                        pins,
                    };
                    if viamind_doc::symbol_package::package_for_symbol(content, id).is_some()
                        && old.is_some_and(|before| !viamind_doc::symbol_package::unit_pose_only(before, &after))
                    {
                        return Err(CommandError::invalid(
                            "symbol",
                            "grouped unit supports pose edits only; edit a detached package definition for new placement",
                        ));
                    }
                    content.symbols.insert(id, after);
                }
                SchematicEdit::Wire { id, start, end } => {
                    let id = existing_id(id, &content.wires, "schematic wire")?;
                    content.wires.insert(id, SchematicWire { id, start, end });
                }
                SchematicEdit::Junction { id, position } => {
                    let id = existing_id(id, &content.junctions, "schematic junction")?;
                    content.junctions.insert(id, SchematicJunction { id, position });
                }
                SchematicEdit::Label { id, position, name, scope } => {
                    let id = existing_id(id, &content.labels, "schematic label")?;
                    content.labels.insert(id, SchematicLabel { id, position, name, scope });
                }
                SchematicEdit::Delete { id } => {
                    if viamind_doc::symbol_package::package_for_symbol(content, id).is_some() {
                        return Err(CommandError::invalid("id", "delete the complete package with delete_symbol_package, not one unit"));
                    }
                    let removed = content.symbols.remove(&id).is_some()
                        || content.wires.remove(&id).is_some()
                        || content.junctions.remove(&id).is_some()
                        || content.labels.remove(&id).is_some();
                    if !removed {
                        return Err(CommandError::not_found("schematic object", id));
                    }
                }
            }
        }
        viamind_doc::schematic::validate_project(project, Some(&sheet)).map_err(|e| CommandError::invalid("schematic", e))?;
        if self.expected.as_ref() == Some(&sheet) {
            return Ok(vec![]);
        }
        Ok(vec![Delta::record_sheet(project, sheet.id, Some(sheet))])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteSchematicSheet {
    pub expected_project: ObjectId,
    pub expected: Sheet,
}
impl Command for DeleteSchematicSheet {
    const NAME: &'static str = "delete_schematic_sheet";
    const DESCRIPTION: &'static str = "Delete a schematic sheet and all its objects in one undoable transaction. Requires exact project ID and complete expected sheet snapshot; stale or missing sheets reject unchanged. Does not affect board objects.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project || project.schematic.sheet(&self.expected.id) != Some(&self.expected) {
            return Err(CommandError::conflict("project or sheet changed; reload before deleting"));
        }
        Ok(vec![Delta::record_sheet(project, self.expected.id, None)])
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SchematicQuery {
    pub sheet_id: Option<ObjectId>,
    pub offset: usize,
    /// Page size 1..100; omitted defaults to 100.
    pub limit: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchematicObject {
    Symbol { symbol: SchematicSymbol },
    Wire { wire: SchematicWire },
    Junction { junction: SchematicJunction },
    Label { label: SchematicLabel },
}
impl SchematicObject {
    pub fn id(&self) -> ObjectId {
        match self {
            Self::Symbol { symbol } => symbol.id,
            Self::Wire { wire } => wire.id,
            Self::Junction { junction } => junction.id,
            Self::Label { label } => label.id,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetSummary {
    pub id: ObjectId,
    pub name: String,
    pub object_count: usize,
    pub pin_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchematicQueryResult {
    pub sheets: Vec<SheetSummary>,
    pub objects: Vec<SchematicObject>,
    pub total: usize,
    pub next_offset: Option<usize>,
}
impl SchematicQuery {
    pub fn run(&self, project: &Project) -> Result<SchematicQueryResult, CommandError> {
        let limit = self.limit.unwrap_or(100);
        if !(1..=100).contains(&limit) {
            return Err(CommandError::invalid("limit", "must be within 1..100"));
        }
        let mut result = SchematicQueryResult { sheets: vec![], objects: vec![], total: 0, next_offset: None };
        if let Some(id) = self.sheet_id {
            let sheet = project.schematic.sheet(&id).ok_or_else(|| CommandError::not_found("sheet", id))?;
            if let Some(c) = &sheet.content {
                let ids: std::collections::BTreeSet<_> =
                    c.symbols.keys().chain(c.wires.keys()).chain(c.junctions.keys()).chain(c.labels.keys()).copied().collect();
                result.total = ids.len();
                result.objects = ids
                    .into_iter()
                    .skip(self.offset)
                    .take(limit)
                    .map(|id| {
                        if let Some(symbol) = c.symbols.get(&id) {
                            SchematicObject::Symbol { symbol: symbol.clone() }
                        } else if let Some(wire) = c.wires.get(&id) {
                            SchematicObject::Wire { wire: wire.clone() }
                        } else if let Some(junction) = c.junctions.get(&id) {
                            SchematicObject::Junction { junction: junction.clone() }
                        } else {
                            SchematicObject::Label { label: c.labels[&id].clone() }
                        }
                    })
                    .collect();
            }
        } else {
            result.total = project.schematic.sheets.len();
            result.sheets = project
                .schematic
                .sheets
                .values()
                .skip(self.offset)
                .take(limit)
                .map(|s| SheetSummary {
                    id: s.id,
                    name: s.name.clone(),
                    object_count: s.content.as_ref().map_or(0, SchematicContent::object_count),
                    pin_count: s.content.as_ref().map_or(0, SchematicContent::pin_count),
                })
                .collect();
        }
        let end = self.offset.saturating_add(limit);
        result.next_offset = (end < result.total).then_some(end);
        Ok(result)
    }
}
impl crate::EngineHandle {
    pub fn query_schematic(&self, input: &SchematicQuery) -> Result<SchematicQueryResult, CommandError> {
        self.read(|project| input.run(project))
    }
}
