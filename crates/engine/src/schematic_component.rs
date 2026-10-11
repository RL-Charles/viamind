//! Explicit, guarded creation from native schematic identity and detached geometry.
use crate::{Command, CommandError, Delta, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use viamind_doc::{ObjectId, Pad, Pads, Part, Project, Side, footprint::Footprint};
use viamind_geom::Point;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicComponentPlacement {
    pub symbol_id: ObjectId,
    pub footprint: Footprint,
    pub position: Point,
    pub side: Side,
    /// 0, 90000, 180000 or 270000 millidegrees.
    pub rotation_mdeg: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicComponentRequest {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub placement: SchematicComponentPlacement,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSchematicPcbComponent {
    pub expected: Project,
    pub source_cursor: EventCursor,
    pub placement: SchematicComponentPlacement,
}
#[derive(Clone, Debug, Serialize)]
pub struct ComponentPadNet {
    pub number: String,
    pub net: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct SchematicComponentPreview {
    pub command: CreateSchematicPcbComponent,
    pub designator: String,
    pub value: String,
    pub pads: Vec<ComponentPadNet>,
}

pub(crate) fn validate_footprint_pose(footprint: &Footprint, position: Point, side: Side, rotation_mdeg: i32) -> Result<(), CommandError> {
    if footprint.pads.len() > 1024 || footprint.name.len() > 4096 || footprint.pads.iter().any(|p| p.number.len() > 4096) {
        return Err(CommandError::invalid("footprint", "at most1024 pads and4096 UTF-8 bytes per name/number before copies"));
    }
    footprint.validate().map_err(|e| CommandError::invalid("footprint", e))?;
    if ![0, 90_000, 180_000, 270_000].contains(&rotation_mdeg) {
        return Err(CommandError::invalid("rotation_mdeg", "choose 0, 90000, 180000 or 270000"));
    }
    if [position.x.0, position.y.0].iter().any(|n| n.unsigned_abs() > 10_000_000_000) {
        return Err(CommandError::invalid("position", "origin must stay within ±10000 mm"));
    }
    // Doubled integer coordinates retain half-nanometre corners of odd pad dimensions.
    for pad in &footprint.pads {
        for dx in [-i128::from(pad.width.0), i128::from(pad.width.0)] {
            for dy in [-i128::from(pad.height.0), i128::from(pad.height.0)] {
                let x = 2 * i128::from(pad.position.x.0) + dx;
                let y = 2 * i128::from(pad.position.y.0) + dy;
                let x = if side == Side::Bottom { -x } else { x };
                let (x, y) = match rotation_mdeg {
                    0 => (x, y),
                    90_000 => (y, -x),
                    180_000 => (-x, -y),
                    270_000 => (-y, x),
                    _ => unreachable!("validated quarter turn"),
                };
                if (x + 2 * i128::from(position.x.0)).abs() > 20_000_000_000 || (y + 2 * i128::from(position.y.0)).abs() > 20_000_000_000 {
                    return Err(CommandError::invalid("position", "transformed pad corners must stay within ±10000 mm"));
                }
            }
        }
    }
    Ok(())
}
fn plan(
    project: &Project,
    placement: &SchematicComponentPlacement,
    cursor: EventCursor,
) -> Result<(String, String, Vec<ComponentPadNet>), CommandError> {
    crate::pcb_sync::validate_sync_project(project)?;
    validate_footprint_pose(&placement.footprint, placement.position, placement.side, placement.rotation_mdeg)?;
    let (sheet_id, symbol) = project
        .schematic
        .sheets
        .values()
        .find_map(|sheet| sheet.content.as_ref()?.symbols.get(&placement.symbol_id).map(|symbol| (sheet.id, symbol)))
        .ok_or_else(|| CommandError::not_found("symbol", placement.symbol_id))?;
    if project.schematic.sheets[&sheet_id].content.as_ref().is_some_and(|c| viamind_doc::symbol_package::package_for_symbol(c, symbol.id).is_some()) {
        return Err(CommandError::invalid(
            "symbol_id",
            "individual package-unit PCB creation is unsupported; package aggregation is required (T116)",
        ));
    }
    if project.board.parts.values().any(|p| p.schematic_symbol == Some(symbol.id)) {
        return Err(CommandError::conflict("symbol already has a linked PCB component"));
    }
    if project.board.parts.values().any(|p| p.designator == symbol.designator) {
        return Err(CommandError::conflict("symbol reference is already used by a PCB component; annotate or resolve explicitly"));
    }
    if symbol.footprint.as_ref().is_some_and(|f| f != &placement.footprint.name) {
        return Err(CommandError::conflict("symbol footprint assignment differs from the chosen library geometry"));
    }
    let pin_numbers: BTreeSet<_> = symbol.pins.iter().map(|p| p.number.as_str()).collect();
    let pad_numbers: BTreeSet<_> = placement.footprint.pads.iter().map(|p| p.number.as_str()).collect();
    if pin_numbers.is_empty() || pin_numbers != pad_numbers || pin_numbers.len() != symbol.pins.len() {
        return Err(CommandError::invalid("footprint", "requires a nonempty exact unambiguous pin/pad-number bijection"));
    }
    let graph = crate::schematic_connectivity::project_schematic_connectivity(project, cursor)?;
    let mut pin_nets = BTreeMap::new();
    for net in graph.nets {
        for member in net.members.iter().filter(|m| m.sheet_id == sheet_id && m.kind == crate::SchematicMemberKind::Pin) {
            pin_nets.insert(member.id, format!("sc:{}", net.id));
        }
    }
    let mut pads = Vec::with_capacity(placement.footprint.pads.len());
    for pad in &placement.footprint.pads {
        let pin = symbol.pins.iter().find(|p| p.number == pad.number).expect("bijective pin numbers");
        let net = pin_nets.get(&pin.id).ok_or_else(|| CommandError::invalid("graph", "pin omitted from complete graph"))?;
        pads.push(ComponentPadNet { number: pad.number.clone(), net: net.clone() });
    }
    Ok((symbol.designator.clone(), symbol.value.clone(), pads))
}
impl crate::EngineHandle {
    /// One immutable bounded capture; graph calculation takes place outside the mutex.
    pub fn preview_schematic_pcb_component(&self, input: SchematicComponentRequest) -> Result<SchematicComponentPreview, CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        let (expected, cursor) = self.schematic_pcb_snapshot()?;
        if expected.id != input.expected_project || cursor != input.source_cursor {
            return Err(CommandError::conflict("document changed; refresh symbols and prepare again"));
        }
        let (designator, value, pads) = plan(&expected, &input.placement, cursor)?;
        let command = CreateSchematicPcbComponent { expected, source_cursor: cursor, placement: input.placement };
        // Check the resulting project budget without adding identities to the preview.
        let mut after = command.expected.clone();
        let provisional = make_part(&command.placement, designator.clone(), value.clone(), &pads)?;
        after.board.parts.insert(provisional.id, provisional);
        crate::pcb_sync::validate_sync_project(&after)?;
        Ok(SchematicComponentPreview { command, designator, value, pads })
    }
}
fn make_part(placement: &SchematicComponentPlacement, designator: String, value: String, nets: &[ComponentPadNet]) -> Result<Part, CommandError> {
    let pads = placement
        .footprint
        .pads
        .iter()
        .zip(nets)
        .map(|(p, net)| Pad {
            id: ObjectId::new(),
            number: p.number.clone(),
            net: Some(net.net.clone()),
            position: Point::new(p.position.x, p.position.y),
            width: p.width,
            height: p.height,
        })
        .collect::<Vec<_>>();
    Ok(Part {
        artwork: placement.footprint.artwork.clone(),
        assembly: None,
        id: ObjectId::new(),
        schematic_symbol: Some(placement.symbol_id),
        schematic_package: None,
        designator,
        value,
        footprint: placement.footprint.name.clone(),
        position: placement.position,
        side: placement.side,
        rotation_mdeg: placement.rotation_mdeg,
        pads: Some(Pads::try_from(pads).map_err(|e| CommandError::invalid("footprint", e))?),
    })
}
impl Command for CreateSchematicPcbComponent {
    const NAME: &'static str = "create_schematic_pcb_component";
    const DESCRIPTION: &'static str = "Create one linked PCB component for an unlinked native schematic symbol from an explicit validated rectangular SMD footprint, integer Nm position, side and quarter-turn rotation. Exact expected project and stream/cursor guards; symbol reference/value copied, unique pin/pad-number bijection derives stable sc: graph nets. Duplicate references/links, mismatched footprint, bounds and stale input reject atomically. Existing components, schematic and copper unchanged. One shared undo. No clearance, fabrication or routing certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        crate::pcb_sync::sync_budget(project)?;
        crate::pcb_sync::sync_budget(&self.expected)?;
        if project != &self.expected {
            return Err(CommandError::conflict("project changed; prepare component creation again"));
        }
        let (reference, value, nets) = plan(project, &self.placement, self.source_cursor)?;
        let part = make_part(&self.placement, reference, value, &nets)?;
        let mut after = project.clone();
        after.board.parts.insert(part.id, part.clone());
        crate::pcb_sync::validate_sync_project(&after)?;
        Ok(vec![Delta::record_part(project, part.id, Some(part))])
    }
}
