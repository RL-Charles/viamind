//! Bounded stable physical terminal identity mapping; no electrical inference.
use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project, Side};
use viamind_geom::Point;

use crate::{CommandError, ComponentLinkQuery, ComponentLinkStatus, EngineHandle, EventCursor, LinkedComponent};

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinPadLinkQuery {
    pub component_id: ObjectId,
    #[serde(default)]
    pub offset: usize,
    /// 1..100, default 100.
    pub limit: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PinPadCoverage {
    Linked,
    Unlinked,
    NoExplicitPads,
    TerminalMismatch,
    EmptyTerminals,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkedPinPad {
    pub number: String,
    pub pin_name: Option<String>,
    pub pin_id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet_id: Option<ObjectId>,
    pub pad_id: Option<ObjectId>,
    pub schematic_position: Option<Point>,
    pub board_position: Option<Point>,
    pub board_side: Option<Side>,
    pub navigable: bool,
    pub unsupported_reasons: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PinPadLinkResult {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub component: LinkedComponent,
    pub coverage: PinPadCoverage,
    pub terminals: Vec<LinkedPinPad>,
    pub total: usize,
    pub next_offset: Option<usize>,
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("pin_pad_links", message)
}
fn bounded(at: Point) -> bool {
    [at.x.0, at.y.0].iter().all(|n| n.unsigned_abs() <= 10_000_000_000)
}
fn preflight(project: &Project) -> Result<(), CommandError> {
    let mut count = project.board.parts.len().saturating_add(project.schematic.sheets.len());
    let mut text = 0usize;
    let mut inspect = |value: &str| {
        text = text.saturating_add(value.len());
        if value.len() > 4096 || text > 2 * 1024 * 1024 {
            Err(invalid("inspected sheet names, references, pin names and terminal numbers exceed 4096 bytes per string or 2 MiB aggregate"))
        } else {
            Ok(())
        }
    };
    if count > 10000 {
        return Err(invalid("at most 10000 combined sheets, symbols, parts, pins and explicit pads are supported"));
    }
    for sheet in project.schematic.sheets.values() {
        inspect(&sheet.name)?;
        if let Some(content) = &sheet.content {
            let (records, _) = viamind_doc::symbol_package::membership_budget(content).map_err(invalid)?;
            count = count.saturating_add(content.symbols.len()).saturating_add(records);
            if count > 10000 {
                return Err(invalid("at most 10000 combined sheets, symbols, parts, pins and explicit pads are supported"));
            }
            for group in content.packages.values() {
                inspect(&group.template_name)?;
                for unit in &group.units {
                    inspect(&unit.key)?;
                    inspect(&unit.name)?;
                }
            }
            for symbol in content.symbols.values() {
                count = count.saturating_add(symbol.pins.len());
                if count > 10000 {
                    return Err(invalid("at most 10000 combined sheets, symbols, parts, pins and explicit pads are supported"));
                }
                inspect(&symbol.designator)?;
                for pin in &symbol.pins {
                    inspect(&pin.number)?;
                    inspect(&pin.name)?;
                }
            }
        }
    }
    for part in project.board.parts.values() {
        inspect(&part.designator)?;
        if let Some(pads) = &part.pads {
            count = count.saturating_add(pads.0.len());
            if count > 10000 {
                return Err(invalid("at most 10000 combined sheets, symbols, parts, pins and explicit pads are supported"));
            }
            for pad in &pads.0 {
                inspect(&pad.number)?;
            }
        }
    }
    // Only after the complete allocation/string budget has been checked.
    let mut ids = BTreeSet::from([project.id]);
    let mut identity = |id: ObjectId| {
        if id.as_uuid().is_nil() || !ids.insert(id) { Err(invalid(format!("nil or duplicate inspected identity {id}"))) } else { Ok(()) }
    };
    let numbers = |values: Vec<&str>| {
        let mut unique = BTreeSet::new();
        for value in values {
            if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) || !unique.insert(value) {
                return Err(invalid("terminal numbers must be nonempty, trimmed, unique and contain no control characters"));
            }
        }
        Ok(())
    };
    for sheet in project.schematic.sheets.values() {
        identity(sheet.id)?;
        if let Some(content) = &sheet.content {
            for group in content.packages.values() {
                identity(group.id)?;
            }
            for symbol in content.symbols.values() {
                identity(symbol.id)?;
                for pin in &symbol.pins {
                    identity(pin.id)?;
                }
                numbers(symbol.pins.iter().map(|p| p.number.as_str()).collect())?;
            }
        }
    }
    for part in project.board.parts.values() {
        identity(part.id)?;
        if let Some(pads) = &part.pads {
            for pad in &pads.0 {
                identity(pad.id)?;
            }
            numbers(pads.0.iter().map(|p| p.number.as_str()).collect())?;
        }
    }
    let symbols: BTreeSet<_> = project.schematic.sheets.values().filter_map(|s| s.content.as_ref()).flat_map(|c| c.symbols.keys()).copied().collect();
    for part in project.board.parts.values() {
        if let Some(link) = part.schematic_symbol
            && ids.contains(&link)
            && !symbols.contains(&link)
        {
            return Err(invalid(format!("part {} links to a nonsymbol inspected identity {link}", part.id)));
        }
    }
    Ok(())
}
impl EngineHandle {
    pub fn query_linked_pin_pads(&self, query: &PinPadLinkQuery) -> Result<PinPadLinkResult, CommandError> {
        let limit = query.limit.unwrap_or(100);
        if !(1..=100).contains(&limit) {
            return Err(invalid("limit must be 1..100"));
        }
        self.with_engine(|engine| {
            let project = engine.project();
            preflight(project)?;
            let component = crate::component_links::mapping(
                project,
                engine.event_cursor(),
                &ComponentLinkQuery { object_ids: vec![query.component_id], ..Default::default() },
                &BTreeSet::from([query.component_id]),
                1,
            )?
            .components
            .into_iter()
            .next()
            .ok_or_else(|| invalid("component_id must identify an existing board part, package or schematic unit"))?;
            let part = component.part_id.and_then(|id| project.board.parts.get(&id));
            let symbol = component
                .sheet_id
                .and_then(|id| project.schematic.sheets.get(&id))
                .and_then(|s| s.content.as_ref())
                .and_then(|c| component.symbol_id.and_then(|id| c.symbols.get(&id)));
            let source = if let Some(part) = part {
                // Missing stable links are reported as unresolved coverage by mapping.
                if component.status == ComponentLinkStatus::Linked { crate::package_component::component_source(project, part)? } else { None }
            } else {
                None
            };
            let pins: BTreeMap<_, _> = if let Some(source) = &source {
                source.members.iter().flat_map(|(u, s)| s.pins.iter().map(move |p| (p.number.as_str(), (*u, *s, p)))).collect()
            } else {
                symbol.into_iter().flat_map(|s| s.pins.iter().map(move |p| (p.number.as_str(), (None, s, p)))).collect()
            };
            let pads: BTreeMap<_, _> = part.and_then(|p| p.pads.as_ref()).into_iter().flat_map(|p| &p.0).map(|p| (p.number.as_str(), p)).collect();
            let coverage = if component.status != ComponentLinkStatus::Linked {
                PinPadCoverage::Unlinked
            } else if part.is_some_and(|p| p.pads.is_none()) {
                PinPadCoverage::NoExplicitPads
            } else if pins.keys().ne(pads.keys()) {
                PinPadCoverage::TerminalMismatch
            } else if pins.is_empty() {
                PinPadCoverage::EmptyTerminals
            } else {
                PinPadCoverage::Linked
            };
            let numbers: BTreeSet<_> = pins.keys().chain(pads.keys()).copied().collect();
            let total = numbers.len();
            let start = query.offset.min(total);
            let end = start.saturating_add(limit).min(total);
            let terminals = numbers
                .into_iter()
                .skip(start)
                .take(end - start)
                .map(|number| {
                    let provenance = pins.get(number).copied();
                    let pin = provenance.map(|(_, _, p)| p);
                    let pad = pads.get(number).copied();
                    let mut reasons = Vec::new();
                    if coverage != PinPadCoverage::Linked {
                        reasons.push(format!("component coverage is {coverage:?}; establish an exact unique pin/pad number bijection"));
                    }
                    if pin.is_none() {
                        reasons.push("No schematic pin has this exact terminal number".into());
                    }
                    if pad.is_none() {
                        reasons.push("No explicit PCB pad has this exact terminal number".into());
                    }
                    let schematic_position = pin.and_then(|pin| {
                        provenance.and_then(|(_, s, _)| {
                            if pin.position.validate().is_err() || s.position.validate().is_err() {
                                return None;
                            }
                            s.pin_position(pin.position).ok().map(|at| Point::new(at.x, at.y))
                        })
                    });
                    let board_position = pad.and_then(|pad| {
                        part.and_then(|p| {
                            if !bounded(p.position)
                                || !bounded(pad.position)
                                || [pad.width.0, pad.height.0].iter().any(|n| *n <= 0 || *n > 10_000_000_000)
                            {
                                return None;
                            }
                            p.local_to_world_nm_exact(pad.position).ok().filter(|p| bounded(*p))
                        })
                    });
                    if pin.is_some() && schematic_position.is_none() {
                        reasons.push("Schematic endpoint transform is invalid, overflows or exceeds ±10000 mm".into());
                    }
                    if pad.is_some() && board_position.is_none() {
                        reasons.push(
                            "PCB pad requires positive bounded dimensions, a quarter-turn rotation and coordinates within ±10000 mm without overflow"
                                .into(),
                        );
                    }
                    LinkedPinPad {
                        number: number.into(),
                        pin_name: pin.map(|p| p.name.clone()),
                        pin_id: pin.map(|p| p.id),
                        package_id: component.package_id,
                        unit_key: provenance.and_then(|(u, _, _)| u.map(|u| u.key.clone())),
                        symbol_id: component.package_id.and_then(|_| provenance.map(|(_, s, _)| s.id)),
                        sheet_id: component.package_id.and(component.sheet_id),
                        pad_id: pad.map(|p| p.id),
                        schematic_position,
                        board_position,
                        board_side: part.map(|p| p.side),
                        navigable: reasons.is_empty(),
                        unsupported_reasons: reasons,
                    }
                })
                .collect();
            Ok(PinPadLinkResult {
                project_id: project.id,
                source_cursor: engine.event_cursor(),
                component,
                coverage,
                terminals,
                total,
                next_offset: (end < total).then_some(end),
            })
        })
    }
}
