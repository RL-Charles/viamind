//! Complete explicit package identity and physical geometry; no reference inference.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use viamind_doc::{
    ObjectId, Pad, Pads, Part, Project, Side,
    footprint::Footprint,
    schematic::SchematicSymbol,
    symbol_package::{PackageUnit, SchematicPackage},
};
use viamind_geom::Point;

pub(crate) struct PackageSource<'a> {
    pub sheet_id: ObjectId,
    pub package: &'a SchematicPackage,
    pub members: Vec<(&'a PackageUnit, &'a SchematicSymbol)>,
}
pub(crate) struct ComponentSource<'a> {
    pub sheet_id: ObjectId,
    pub package_id: Option<ObjectId>,
    pub members: Vec<(Option<&'a PackageUnit>, &'a SchematicSymbol)>,
}
pub(crate) fn component_source<'a>(project: &'a Project, part: &Part) -> Result<Option<ComponentSource<'a>>, CommandError> {
    part.validate_schematic_links().map_err(|e| CommandError::invalid("part", e))?;
    if let Some(id) = part.schematic_package {
        let source = package_source(project, id)?;
        return Ok(Some(ComponentSource {
            sheet_id: source.sheet_id,
            package_id: Some(id),
            members: source.members.into_iter().map(|(u, s)| (Some(u), s)).collect(),
        }));
    }
    if let Some(id) = part.schematic_symbol {
        let (sheet_id, symbol) = project
            .schematic
            .sheets
            .values()
            .find_map(|s| s.content.as_ref()?.symbols.get(&id).map(|symbol| (s.id, symbol)))
            .ok_or_else(|| CommandError::conflict(format!("part {} has unresolved symbol link {id}; resolve or explicitly unlink", part.id)))?;
        return Ok(Some(ComponentSource { sheet_id, package_id: None, members: vec![(None, symbol)] }));
    }
    Ok(None)
}
/// Package associations require all physical terminals before synchronization.
pub(crate) fn validate_component_terminals(source: &ComponentSource<'_>, part: &Part) -> Result<(), CommandError> {
    let pins: BTreeSet<_> = source.members.iter().flat_map(|(_, s)| s.pins.iter().map(|p| p.number.as_str())).collect();
    let count: usize = source.members.iter().map(|(_, s)| s.pins.len()).sum();
    let pads = part.pads.as_ref().ok_or_else(|| CommandError::conflict("complete package synchronization requires explicit numbered pads"))?;
    let numbers: BTreeSet<_> = pads.0.iter().map(|p| p.number.as_str()).collect();
    if pins.is_empty() || pins != numbers || pins.len() != count || numbers.len() != pads.0.len() {
        return Err(CommandError::conflict("complete package synchronization requires all unit pins and pads in an exact unique bijection"));
    }
    Ok(())
}
pub(crate) fn package_source(project: &Project, id: ObjectId) -> Result<PackageSource<'_>, CommandError> {
    let (sheet, content, package) = project
        .schematic
        .sheets
        .values()
        .find_map(|sheet| {
            let c = sheet.content.as_ref()?;
            c.packages.get(&id).map(|g| (sheet, c, g))
        })
        .ok_or_else(|| CommandError::conflict(format!("package {id} is missing; resolve or explicitly unlink its stable identity")))?;
    viamind_doc::symbol_package::membership_budget(content).map_err(|e| CommandError::invalid("package", e))?;
    package.validate(content).map_err(|e| CommandError::invalid("package", e))?;
    Ok(PackageSource { sheet_id: sheet.id, package, members: package.units.iter().map(|u| (u, &content.symbols[&u.symbol_id])).collect() })
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageComponentPlacement {
    pub package_id: ObjectId,
    pub footprint: Footprint,
    pub position: Point,
    pub side: Side,
    pub rotation_mdeg: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageComponentRequest {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub placement: PackageComponentPlacement,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePackagePcbComponent {
    pub expected: Project,
    pub source_cursor: EventCursor,
    pub placement: PackageComponentPlacement,
}
#[derive(Clone, Debug, Serialize)]
pub struct PackageComponentPin {
    pub number: String,
    pub unit_key: String,
    pub symbol_id: ObjectId,
    pub pin_id: ObjectId,
    pub net: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PackageComponentPreview {
    pub command: CreatePackagePcbComponent,
    pub designator: String,
    pub value: String,
    pub unit_count: usize,
    pub pins: Vec<PackageComponentPin>,
}
struct Plan {
    designator: String,
    value: String,
    unit_count: usize,
    pins: Vec<PackageComponentPin>,
}
fn plan(project: &Project, placement: &PackageComponentPlacement, cursor: EventCursor) -> Result<Plan, CommandError> {
    crate::pcb_sync::validate_sync_project(project)?;
    crate::schematic_component::validate_footprint_pose(&placement.footprint, placement.position, placement.side, placement.rotation_mdeg)?;
    let source = package_source(project, placement.package_id)?;
    if project.board.parts.values().any(|p| p.schematic_package == Some(placement.package_id)) {
        return Err(CommandError::conflict("complete package already has a linked physical component"));
    }
    let shared = source.members[0].1;
    if project.board.parts.values().any(|p| p.designator == shared.designator) {
        return Err(CommandError::conflict("package reference already belongs to a physical component; resolve explicitly"));
    }
    if shared.footprint.as_ref().is_some_and(|f| f != &placement.footprint.name) {
        return Err(CommandError::conflict("package footprint assignment differs from chosen explicit geometry"));
    }
    let pins: BTreeMap<_, _> = source.members.iter().flat_map(|(u, s)| s.pins.iter().map(move |p| (p.number.as_str(), (*u, *s, p)))).collect();
    let numbers: BTreeSet<_> = placement.footprint.pads.iter().map(|p| p.number.as_str()).collect();
    if pins.is_empty() || pins.keys().copied().collect::<BTreeSet<_>>() != numbers {
        return Err(CommandError::invalid("footprint", "all package pins require an exact complete unique pad-number bijection"));
    }
    let graph = crate::schematic_connectivity::project_schematic_connectivity(project, cursor)?;
    let mut nets = BTreeMap::new();
    for net in graph.nets {
        for member in net.members.iter().filter(|m| m.sheet_id == source.sheet_id && m.kind == crate::SchematicMemberKind::Pin) {
            nets.insert(member.id, format!("sc:{}", net.id));
        }
    }
    let mut result = Vec::with_capacity(placement.footprint.pads.len());
    for pad in &placement.footprint.pads {
        let (unit, symbol, pin) = pins[pad.number.as_str()];
        let net = nets.get(&pin.id).ok_or_else(|| CommandError::invalid("graph", "complete package pin missing from graph"))?;
        result.push(PackageComponentPin {
            number: pad.number.clone(),
            unit_key: unit.key.clone(),
            symbol_id: symbol.id,
            pin_id: pin.id,
            net: net.clone(),
        });
    }
    Ok(Plan { designator: shared.designator.clone(), value: shared.value.clone(), unit_count: source.package.units.len(), pins: result })
}
fn make_part(placement: &PackageComponentPlacement, plan: &Plan) -> Result<Part, CommandError> {
    let pads = placement
        .footprint
        .pads
        .iter()
        .zip(&plan.pins)
        .map(|(p, pin)| Pad {
            id: ObjectId::new(),
            number: p.number.clone(),
            net: Some(pin.net.clone()),
            position: Point::new(p.position.x, p.position.y),
            width: p.width,
            height: p.height,
        })
        .collect::<Vec<_>>();
    Ok(Part {
        assembly: None,
        schematic_symbol: None,
        schematic_package: Some(placement.package_id),
        pads: Some(Pads::try_from(pads).map_err(|e| CommandError::invalid("footprint", e))?),
        id: ObjectId::new(),
        designator: plan.designator.clone(),
        value: plan.value.clone(),
        footprint: placement.footprint.name.clone(),
        position: placement.position,
        rotation_mdeg: placement.rotation_mdeg,
        side: placement.side,
    })
}
impl EngineHandle {
    pub fn preview_package_pcb_component(&self, input: PackageComponentRequest) -> Result<PackageComponentPreview, CommandError> {
        crate::schematic_component::validate_footprint_pose(
            &input.placement.footprint,
            input.placement.position,
            input.placement.side,
            input.placement.rotation_mdeg,
        )?;
        let (expected, cursor) = self.schematic_pcb_snapshot()?;
        if expected.id != input.expected_project || cursor != input.source_cursor {
            return Err(CommandError::conflict("document changed; refresh complete packages before preparing"));
        }
        let plan = plan(&expected, &input.placement, cursor)?;
        let command = CreatePackagePcbComponent { expected, source_cursor: cursor, placement: input.placement };
        let mut after = command.expected.clone();
        let provisional = make_part(&command.placement, &plan)?;
        after.board.parts.insert(provisional.id, provisional);
        crate::pcb_sync::validate_sync_project(&after)?;
        Ok(PackageComponentPreview { command, designator: plan.designator, value: plan.value, unit_count: plan.unit_count, pins: plan.pins })
    }
}
impl Command for CreatePackagePcbComponent {
    const NAME: &'static str = "create_package_pcb_component";
    const DESCRIPTION: &'static str = "Create one physical PCB component from one complete explicit native schematic package UUID. Exact bounded whole-project/full-cursor guard, explicit rectangular SMD footprint and integer-Nm origin/side/cardinal rotation. All units' unique full pin-number union must biject all pads; actual pin UUID graph nets assigned, no role/reference inference. Missing/partial/duplicate/stale/mismatched or out-of-bounds proposals refuse atomically. One Part delta/undo/redo preserves package/member/pin identity and copper. Placement clearance requires subsequent DRC; no certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        crate::pcb_sync::sync_budget(project)?;
        crate::pcb_sync::sync_budget(&self.expected)?;
        if project != &self.expected {
            return Err(CommandError::conflict("project changed; prepare complete package creation again"));
        }
        let plan = plan(project, &self.placement, self.source_cursor)?;
        let part = make_part(&self.placement, &plan)?;
        let mut after = project.clone();
        after.board.parts.insert(part.id, part.clone());
        crate::pcb_sync::validate_sync_project(&after)?;
        Ok(vec![Delta::record_part(project, part.id, Some(part))])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkSchematicPackage {
    pub expected: Project,
    pub source_cursor: EventCursor,
    pub part_id: ObjectId,
    /// None explicitly unlinks a complete package; symbol links are a separate operation.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub package_id: Option<ObjectId>,
}
impl Command for LinkSchematicPackage {
    const NAME: &'static str = "link_schematic_package";
    const DESCRIPTION: &'static str = "Explicitly link one existing physical Part to a complete native package UUID, or clear its package link with package_id=null. Exact bounded whole-project/full-cursor guard; one physical owner and complete unique all-unit pin/pad bijection plus matching footprint required. Refuses single-symbol ownership, stale/missing/partial targets and duplicate ownership. Keeps pose, geometry, assembly and every pad net; no inferred relinking. One undoable Part delta.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        crate::pcb_sync::validate_sync_project(project)?;
        crate::pcb_sync::validate_sync_project(&self.expected)?;
        if project != &self.expected {
            return Err(CommandError::conflict("project changed; refresh package linking guard"));
        }
        let part = project.board.parts.get(&self.part_id).ok_or_else(|| CommandError::not_found("part", self.part_id))?;
        if part.schematic_symbol.is_some() {
            return Err(CommandError::conflict("part already has a single-symbol link; explicitly unlink it first"));
        }
        if let Some(id) = self.package_id {
            let source = package_source(project, id)?;
            if project.board.parts.values().any(|p| p.id != part.id && p.schematic_package == Some(id)) {
                return Err(CommandError::conflict("package already linked to another physical component"));
            }
            if part.schematic_package.is_some_and(|old| old != id) {
                return Err(CommandError::conflict("part already links to another package; explicitly unlink it first"));
            }
            let shared = source.members[0].1;
            if shared.footprint.as_ref().is_some_and(|f| f != &part.footprint) {
                return Err(CommandError::conflict("package/part footprint assignments differ"));
            }
            let pins: BTreeSet<_> = source.members.iter().flat_map(|(_, s)| s.pins.iter().map(|p| p.number.as_str())).collect();
            let pads = part.pads.as_ref().ok_or_else(|| CommandError::invalid("part", "complete package linking requires explicit numbered pads"))?;
            let numbers: BTreeSet<_> = pads.0.iter().map(|p| p.number.as_str()).collect();
            if pins.is_empty() || pins != numbers || numbers.len() != pads.0.len() {
                return Err(CommandError::invalid("part", "exact complete all-unit pin/pad-number bijection required"));
            }
        }
        if part.schematic_package == self.package_id {
            return Ok(vec![]);
        }
        let mut after = part.clone();
        after.schematic_package = self.package_id;
        Ok(vec![Delta::record_part(project, part.id, Some(after))])
    }
}
