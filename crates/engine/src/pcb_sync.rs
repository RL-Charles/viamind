//! Stable schematic/PCB links and bounded guarded synchronization.
use crate::{Command, CommandError, Delta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Part, Project, schematic::SchematicSymbol};

/// Explicitly establish or remove a symbol link without reference-based inference.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchematicLinkTarget {
    Unlink,
    Symbol { expected: SchematicSymbol },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkSchematicSymbol {
    pub expected_project: ObjectId,
    pub expected_part: Part,
    /// Explicit unlink or exact current symbol snapshot.
    pub target: SchematicLinkTarget,
}
impl Command for LinkSchematicSymbol {
    const NAME: &'static str = "link_schematic_symbol";
    const DESCRIPTION: &'static str = "Link an existing PCB part to an existing schematic symbol by stable UUID, or explicitly unlink with target.kind=unlink. Requires exact project, part and current symbol guards; one symbol can link to at most one part. No reference-based inference or geometry/net edits. One undoable transaction; bounded native synchronization scope.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        sync_budget(project)?;
        viamind_doc::schematic::validate_project(project, None).map_err(|e| CommandError::invalid("project", e))?;
        if self.expected_project != project.id || project.board.parts.get(&self.expected_part.id) != Some(&self.expected_part) {
            return Err(CommandError::conflict("project or part changed; reload before linking"));
        }
        if self.expected_part.schematic_package.is_some() {
            return Err(CommandError::invalid("part", "component is linked to a complete package; use explicit package unlink first"));
        }
        let symbol_id = if let SchematicLinkTarget::Symbol { expected } = &self.target {
            let current = project.schematic.sheets.values().filter_map(|s| s.content.as_ref()).find_map(|c| c.symbols.get(&expected.id));
            if current != Some(expected) {
                return Err(CommandError::conflict("symbol changed or missing; reload before linking"));
            }
            if expected.id.as_uuid().is_nil() {
                return Err(CommandError::invalid("target", "symbol identity must be nonnil"));
            }
            if project.board.parts.values().any(|p| p.id != self.expected_part.id && p.schematic_symbol == Some(expected.id)) {
                return Err(CommandError::conflict("symbol already linked to another board part; unlink it explicitly first"));
            }
            if project
                .schematic
                .sheets
                .values()
                .filter_map(|s| s.content.as_ref())
                .any(|c| viamind_doc::symbol_package::package_for_symbol(c, expected.id).is_some())
            {
                return Err(CommandError::invalid(
                    "target",
                    "individual package-unit PCB links are unsupported; package aggregation is required (T116)",
                ));
            }
            Some(expected.id)
        } else {
            None
        };
        let mut after = self.expected_part;
        if after.schematic_symbol == symbol_id {
            return Ok(vec![]);
        }
        after.schematic_symbol = symbol_id;
        Ok(vec![Delta::record_part(project, after.id, Some(after))])
    }
}

/// Check counts and string sizes before any whole-project synchronization copy.
pub(crate) fn sync_budget(project: &Project) -> Result<(), CommandError> {
    viamind_doc::file::validate_assembly(project).map_err(|e| CommandError::invalid("assembly", e.to_string()))?;
    const MAX_OBJECTS: usize = 10_000;
    const MAX_TEXT: usize = 4 * 1024 * 1024;
    if project.id.as_uuid().is_nil() {
        return Err(CommandError::invalid("project", "project identity must be nonnil"));
    }
    let mut count = project
        .board
        .parts
        .len()
        .saturating_add(project.board.tracks.len())
        .saturating_add(project.board.vias.len())
        .saturating_add(project.schematic.sheets.len());
    let mut count_more = |n: usize| -> Result<(), CommandError> {
        count = count.saturating_add(n);
        if count > MAX_OBJECTS {
            return Err(CommandError::invalid("project", "synchronization supports at most 10000 objects including pins, pads and outline vertices"));
        }
        Ok(())
    };
    count_more(0)?;
    for p in project.board.parts.values() {
        count_more(p.pads.as_ref().map_or(0, |p| p.0.len()))?;
    }
    if let Some(outline) = &project.board.outline {
        for ring in outline.rings() {
            count_more(ring.points.len())?;
        }
    }
    for sheet in project.schematic.sheets.values() {
        if let Some(c) = &sheet.content {
            let (members, _) = viamind_doc::symbol_package::membership_budget(c).map_err(|e| CommandError::invalid("project", e))?;
            count_more(c.object_count().saturating_add(members.saturating_sub(c.packages.len())))?;
            for symbol in c.symbols.values() {
                count_more(symbol.pins.len())?;
            }
        }
    }
    let mut text = 0usize;
    let mut add = |s: &str| -> Result<(), CommandError> {
        text = text.saturating_add(s.len());
        if s.len() > 4096 || text > MAX_TEXT {
            return Err(CommandError::invalid("project", "synchronization text exceeds 4096 bytes per string or 4 MiB aggregate"));
        }
        Ok(())
    };
    add(&project.name)?;
    for p in project.board.parts.values() {
        for s in [&p.designator, &p.value, &p.footprint] {
            add(s)?;
        }
        for pad in p.pads.iter().flat_map(|p| &p.0) {
            add(&pad.number)?;
            if let Some(net) = &pad.net {
                add(net)?;
            }
        }
    }
    for t in project.board.tracks.values() {
        if let Some(net) = &t.net {
            add(net)?;
        }
    }
    for v in project.board.vias.values() {
        if let Some(net) = &v.net {
            add(net)?;
        }
    }
    for sheet in project.schematic.sheets.values() {
        add(&sheet.name)?;
        if let Some(c) = &sheet.content {
            for group in c.packages.values() {
                add(&group.template_name)?;
                for unit in &group.units {
                    add(&unit.key)?;
                    add(&unit.name)?;
                }
            }
            for symbol in c.symbols.values() {
                add(&symbol.designator)?;
                add(&symbol.value)?;
                if let Some(f) = &symbol.footprint {
                    add(f)?;
                }
                for pin in &symbol.pins {
                    add(&pin.name)?;
                    add(&pin.number)?;
                }
            }
            for label in c.labels.values() {
                add(&label.name)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PcbSyncDirection {
    Forward,
    Back,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbSyncRequest {
    pub direction: PcbSyncDirection,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynchronizeSchematicPcb {
    pub expected: Project,
    pub source_cursor: crate::EventCursor,
    pub direction: PcbSyncDirection,
}
#[derive(Clone, Debug, Serialize)]
pub struct PcbSyncChange {
    pub sheet_id: ObjectId,
    pub symbol_id: ObjectId,
    pub part_id: ObjectId,
    pub pad_id: Option<ObjectId>,
    pub field: &'static str,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PcbSyncNet {
    pub board_net: String,
    pub canonical_name: Option<String>,
    pub aliases: Vec<crate::SchematicNetName>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PcbSyncPreview {
    pub command: SynchronizeSchematicPcb,
    pub changes: Vec<PcbSyncChange>,
    pub nets: Vec<PcbSyncNet>,
    pub unlinked_symbols: Vec<ObjectId>,
    pub unlinked_parts: Vec<ObjectId>,
    pub exclusions: Vec<&'static str>,
}
impl crate::EngineHandle {
    /// Bounded immutable link/preview input; detailed validation runs after releasing the lock.
    pub fn schematic_pcb_snapshot(&self) -> Result<(Project, crate::EventCursor), CommandError> {
        let (project, cursor) = self.with_engine(|e| {
            sync_budget(e.project())?;
            Ok::<_, CommandError>((e.project().clone(), e.event_cursor()))
        })?;
        validate_sync_project(&project)?;
        Ok((project, cursor))
    }
    /// Capture once under lock, then calculate the immutable preview outside it.
    pub fn preview_schematic_pcb_sync(&self, direction: PcbSyncDirection) -> Result<PcbSyncPreview, CommandError> {
        let (expected, source_cursor) = self.schematic_pcb_snapshot()?;
        let command = SynchronizeSchematicPcb { expected, source_cursor, direction };
        let plan = calculate_sync(&command.expected, direction, source_cursor)?;
        Ok(PcbSyncPreview {
            command,
            changes: plan.changes,
            nets: plan.nets,
            unlinked_symbols: plan.unlinked_symbols,
            unlinked_parts: plan.unlinked_parts,
            exclusions: vec![
                "unlinked parts and symbols remain unchanged",
                "part creation/removal, inferred links and footprint geometry replacement",
                "routed net reassignment, reverse net names and topology",
                "hierarchy, buses and hidden power pins absent from the native graph",
            ],
        })
    }
}
pub(crate) fn validate_sync_project(project: &Project) -> Result<(), CommandError> {
    sync_budget(project)?;
    // Structural/text limits precede this allocation; this also bounds command output.
    if serde_json::to_vec(project).map_err(|e| CommandError::invalid("project", e.to_string()))?.len() > 8 * 1024 * 1024 {
        return Err(CommandError::invalid("project", "serialized synchronization guard exceeds 8 MiB"));
    }
    viamind_doc::file::validate_pads(project).map_err(|e| CommandError::invalid("project", e.to_string()))?;
    viamind_doc::file::validate_copper(project).map_err(|e| CommandError::invalid("project", e.to_string()))?;
    viamind_doc::schematic::validate_project(project, None).map_err(|e| CommandError::invalid("project", e))
}
struct SyncPlan {
    after: Project,
    changes: Vec<PcbSyncChange>,
    nets: Vec<PcbSyncNet>,
    unlinked_symbols: Vec<ObjectId>,
    unlinked_parts: Vec<ObjectId>,
}
fn calculate_sync(project: &Project, direction: PcbSyncDirection, cursor: crate::EventCursor) -> Result<SyncPlan, CommandError> {
    use std::collections::{BTreeMap, BTreeSet};
    validate_sync_project(project)?;
    if project.schematic.sheets.values().filter_map(|s| s.content.as_ref()).any(|c| !c.packages.is_empty()) {
        return Err(CommandError::invalid(
            "project",
            "multi-unit package PCB synchronization is unsupported; package aggregation is required (T116)",
        ));
    }
    let graph = crate::schematic_connectivity::project_schematic_connectivity(project, cursor)?;
    let mut pin_nets = BTreeMap::new();
    let mut nets = Vec::new();
    for net in graph.nets {
        let board_net = format!("sc:{}", net.id);
        for pin in net.members.iter().filter(|m| m.kind == crate::SchematicMemberKind::Pin) {
            pin_nets.insert((pin.sheet_id, pin.id), board_net.clone());
        }
        nets.push(PcbSyncNet { board_net, canonical_name: net.canonical_name, aliases: net.names });
    }
    let symbols: BTreeMap<_, _> = project
        .schematic
        .sheets
        .values()
        .flat_map(|sheet| sheet.content.iter().flat_map(move |c| c.symbols.values().map(move |symbol| (symbol.id, (sheet.id, symbol)))))
        .collect();
    let linked: BTreeSet<_> = project.board.parts.values().filter_map(|p| p.schematic_symbol).collect();
    let unlinked_symbols = symbols.keys().filter(|id| !linked.contains(*id)).copied().collect();
    let unlinked_parts = project.board.parts.values().filter(|p| p.schematic_symbol.is_none()).map(|p| p.id).collect();
    let copper_nets: BTreeSet<_> =
        project.board.tracks.values().filter_map(|t| t.net.as_deref()).chain(project.board.vias.values().filter_map(|v| v.net.as_deref())).collect();
    let mut after = project.clone();
    let mut changes = Vec::new();
    for part in project.board.parts.values() {
        let Some(symbol_id) = part.schematic_symbol else { continue };
        let (sheet_id, symbol) = symbols.get(&symbol_id).ok_or_else(|| {
            CommandError::conflict(format!("part {} has an unresolved symbol link {}; unlink or resolve explicitly", part.id, symbol_id))
        })?;
        let mut record = |field, pad_id, before: Option<String>, value: Option<String>| {
            if before != value {
                changes.push(PcbSyncChange { sheet_id: *sheet_id, symbol_id, part_id: part.id, pad_id, field, before, after: value });
            }
        };
        match direction {
            PcbSyncDirection::Forward => {
                if symbol.footprint.as_ref().is_some_and(|f| f != &part.footprint) {
                    return Err(CommandError::conflict(format!(
                        "part {} footprint assignment differs; explicitly apply matching geometry before synchronizing",
                        part.id
                    )));
                }
                let pads = part
                    .pads
                    .as_ref()
                    .ok_or_else(|| CommandError::conflict(format!("part {} requires explicit numbered pads before forward annotation", part.id)))?;
                let pin_numbers: BTreeSet<_> = symbol.pins.iter().map(|p| &p.number).collect();
                let pad_numbers: BTreeSet<_> = pads.0.iter().map(|p| &p.number).collect();
                if pin_numbers != pad_numbers || pin_numbers.len() != symbol.pins.len() || pad_numbers.len() != pads.0.len() {
                    return Err(CommandError::conflict(format!("part {} requires an exact unambiguous pin/pad-number bijection", part.id)));
                }
                let target = after.board.parts.get_mut(&part.id).expect("captured part");
                record("designator", None, Some(part.designator.clone()), Some(symbol.designator.clone()));
                record("value", None, Some(part.value.clone()), Some(symbol.value.clone()));
                target.designator = symbol.designator.clone();
                target.value = symbol.value.clone();
                for pad in &mut target.pads.as_mut().expect("explicit pads").0 {
                    let pin = symbol.pins.iter().find(|p| p.number == pad.number).expect("bijective pin numbers");
                    let net = pin_nets.get(&(*sheet_id, pin.id)).ok_or_else(|| CommandError::invalid("graph", "pin omitted from complete graph"))?;
                    let value = Some(net.clone());
                    if pad.net != value && (pad.net.as_deref().is_some_and(|n| copper_nets.contains(n)) || copper_nets.contains(net.as_str())) {
                        return Err(CommandError::conflict(format!(
                            "pad {} net change touches existing routed copper; explicit routed-net migration is required",
                            pad.id
                        )));
                    }
                    record("pad_net", Some(pad.id), pad.net.clone(), value.clone());
                    pad.net = value;
                }
            }
            PcbSyncDirection::Back => {
                let target = after
                    .schematic
                    .sheets
                    .get_mut(sheet_id)
                    .expect("captured sheet")
                    .content
                    .as_mut()
                    .expect("captured content")
                    .symbols
                    .get_mut(&symbol_id)
                    .expect("captured symbol");
                record("designator", None, Some(symbol.designator.clone()), Some(part.designator.clone()));
                record("value", None, Some(symbol.value.clone()), Some(part.value.clone()));
                record("footprint", None, symbol.footprint.clone(), Some(part.footprint.clone()));
                target.designator = part.designator.clone();
                target.value = part.value.clone();
                target.footprint = Some(part.footprint.clone());
            }
        }
    }
    let mut board_refs = BTreeSet::new();
    for part in after.board.parts.values() {
        if !board_refs.insert(&part.designator) {
            return Err(CommandError::conflict("synchronization would duplicate a board reference"));
        }
    }
    validate_sync_project(&after)?;
    Ok(SyncPlan { after, changes, nets, unlinked_symbols, unlinked_parts })
}
impl Command for SynchronizeSchematicPcb {
    const NAME: &'static str = "synchronize_schematic_pcb";
    const DESCRIPTION: &'static str = "Apply a bounded exact-project/cursor guarded schematic-PCB synchronization preview. Forward copies linked references/values and exact graph pin-to-pad nets; back copies linked references/values/footprint assignments. Stable UUID links, explicit pin/pad bijection, geometry and copper preserved. Missing links, mismatched footprints, collisions, routed-net reassignment or stale stream/sequence reject atomically. Unlinked objects unchanged. No creation/deletion/hierarchy/reverse topology. One shared undo transaction.";
    fn expected_cursor(&self) -> Option<crate::EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        sync_budget(project)?;
        sync_budget(&self.expected)?;
        if *project != self.expected {
            return Err(CommandError::conflict("project changed; prepare synchronization again"));
        }
        let plan = calculate_sync(project, self.direction, self.source_cursor)?;
        let mut deltas = Vec::new();
        for (id, part) in plan.after.board.parts {
            if project.board.parts.get(&id) != Some(&part) {
                deltas.push(Delta::record_part(project, id, Some(part)));
            }
        }
        for (id, sheet) in plan.after.schematic.sheets {
            if project.schematic.sheets.get(&id) != Some(&sheet) {
                deltas.push(Delta::record_sheet(project, id, Some(sheet)));
            }
        }
        Ok(deltas)
    }
}
