//! Bounded read-only component mapping from explicit stable schematic links.
use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project};
use viamind_geom::Point;

use crate::{CommandError, EngineHandle, EventCursor};

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentLinkQuery {
    #[serde(default)]
    pub object_ids: Vec<ObjectId>,
    #[serde(default)]
    pub offset: usize,
    /// 1..100, default 100.
    pub limit: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentLinkStatus {
    Linked,
    UnlinkedPart,
    UnlinkedSymbol,
    UnresolvedSymbol,
}
#[derive(Clone, Debug, Serialize)]
pub struct LinkedComponent {
    pub part_id: Option<ObjectId>,
    pub symbol_id: Option<ObjectId>,
    pub sheet_id: Option<ObjectId>,
    pub part_reference: Option<String>,
    pub symbol_reference: Option<String>,
    pub board_position: Option<Point>,
    pub schematic_position: Option<Point>,
    pub board_navigable: bool,
    pub schematic_navigable: bool,
    pub status: ComponentLinkStatus,
}
#[derive(Clone, Debug, Serialize)]
pub struct ComponentLinkResult {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub components: Vec<LinkedComponent>,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub unknown_ids: Vec<ObjectId>,
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("component_links", message)
}
fn navigable(at: Point) -> bool {
    [at.x.0, at.y.0].iter().all(|n| n.unsigned_abs() <= 10_000_000_000)
}
impl EngineHandle {
    /// Capture identities and positions once; no model/history/selection mutation.
    pub fn query_linked_components(&self, query: &ComponentLinkQuery) -> Result<ComponentLinkResult, CommandError> {
        let limit = query.limit.unwrap_or(100);
        if !(1..=100).contains(&limit) || query.object_ids.len() > 100 {
            return Err(invalid("limit must be 1..100 and object_ids must contain at most 100 identities"));
        }
        let requested: BTreeSet<_> = query.object_ids.iter().copied().collect();
        if requested.len() != query.object_ids.len() {
            return Err(invalid("object_ids must be unique"));
        }
        self.with_engine(|engine| mapping(engine.project(), engine.event_cursor(), query, &requested, limit))
    }
}
pub(crate) fn mapping(
    project: &Project,
    source_cursor: EventCursor,
    query: &ComponentLinkQuery,
    requested: &BTreeSet<ObjectId>,
    limit: usize,
) -> Result<ComponentLinkResult, CommandError> {
    let mut count = project.board.parts.len().saturating_add(project.schematic.sheets.len());
    if count > 10000 {
        return Err(invalid("mapping supports at most 10000 combined sheets, symbols and board parts"));
    }
    let mut grouped = BTreeSet::new();
    for sheet in project.schematic.sheets.values() {
        if let Some(c) = &sheet.content {
            let (records, _) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            count = count.saturating_add(records);
        }
        count = count.saturating_add(sheet.content.as_ref().map_or(0, |c| c.symbols.len()));
        if count > 10000 {
            return Err(invalid("mapping supports at most 10000 combined sheets, symbols and board parts"));
        }
    }
    let mut ids = BTreeSet::new();
    let mut identity = |key: ObjectId, id: ObjectId| {
        if key != id || id.as_uuid().is_nil() || !ids.insert(id) {
            Err(invalid(format!("nil, duplicate or key-mismatched component identity {id}")))
        } else {
            Ok(())
        }
    };
    identity(project.id, project.id)?;
    let mut text = 0usize;
    let mut reference = |value: &str| {
        text = text.saturating_add(value.len());
        if value.len() > 4096 || text > 2 * 1024 * 1024 { Err(invalid("references exceed 4096 bytes per string or 2 MiB aggregate")) } else { Ok(()) }
    };
    let mut symbols = BTreeMap::new();
    for (key, sheet) in &project.schematic.sheets {
        identity(*key, sheet.id)?;
        if let Some(content) = &sheet.content {
            for (key, group) in &content.packages {
                identity(*key, group.id)?;
                reference(&group.template_name)?;
                for unit in &group.units {
                    reference(&unit.key)?;
                    reference(&unit.name)?;
                    grouped.insert(unit.symbol_id);
                }
            }
            for (key, symbol) in &content.symbols {
                identity(*key, symbol.id)?;
                reference(&symbol.designator)?;
                symbols.insert(symbol.id, (sheet.id, symbol));
            }
        }
    }
    let mut linked = BTreeSet::new();
    for (key, part) in &project.board.parts {
        if part.schematic_symbol.is_some_and(|id| grouped.contains(&id)) {
            return Err(invalid("individual package-unit PCB links are unsupported; package aggregation is required"));
        }
        identity(*key, part.id)?;
        reference(&part.designator)?;
        if let Some(symbol) = part.schematic_symbol
            && (symbol.as_uuid().is_nil() || !linked.insert(symbol))
        {
            return Err(invalid(format!("nil or multiply linked symbol {symbol}; repair explicit links")));
        }
    }
    let mut known = BTreeSet::new();
    let mut rows = BTreeMap::new();
    for part in project.board.parts.values() {
        known.insert(part.id);
        let symbol = part.schematic_symbol.and_then(|id| symbols.get(&id));
        if let Some(id) = part.schematic_symbol
            && symbol.is_none()
            && ids.contains(&id)
        {
            return Err(invalid(format!("part {} links to nonsymbol identity {id}", part.id)));
        }
        let position = symbol.map(|(_, s)| Point::new(s.position.x, s.position.y));
        rows.insert(
            part.id,
            LinkedComponent {
                part_id: Some(part.id),
                symbol_id: part.schematic_symbol,
                sheet_id: symbol.map(|(sheet, _)| *sheet),
                part_reference: Some(part.designator.clone()),
                symbol_reference: symbol.map(|(_, s)| s.designator.clone()),
                board_position: Some(part.position),
                schematic_position: position,
                board_navigable: navigable(part.position),
                schematic_navigable: position.is_some_and(navigable),
                status: match (part.schematic_symbol, symbol) {
                    (None, _) => ComponentLinkStatus::UnlinkedPart,
                    (Some(_), None) => ComponentLinkStatus::UnresolvedSymbol,
                    (Some(_), Some(_)) => ComponentLinkStatus::Linked,
                },
            },
        );
    }
    for (id, (sheet, symbol)) in symbols {
        known.insert(id);
        if !linked.contains(&id) {
            let at = Point::new(symbol.position.x, symbol.position.y);
            rows.insert(
                id,
                LinkedComponent {
                    part_id: None,
                    symbol_id: Some(id),
                    sheet_id: Some(sheet),
                    part_reference: None,
                    symbol_reference: Some(symbol.designator.clone()),
                    board_position: None,
                    schematic_position: Some(at),
                    board_navigable: false,
                    schematic_navigable: navigable(at),
                    status: ComponentLinkStatus::UnlinkedSymbol,
                },
            );
        }
    }
    let unknown_ids = requested.difference(&known).copied().collect();
    let rows: Vec<_> = rows
        .into_values()
        .filter(|row| {
            requested.is_empty()
                || row.part_id.is_some_and(|id| requested.contains(&id))
                || (row.status != ComponentLinkStatus::UnresolvedSymbol && row.symbol_id.is_some_and(|id| requested.contains(&id)))
        })
        .collect();
    let total = rows.len();
    let start = query.offset.min(total);
    let end = start.saturating_add(limit).min(total);
    let components = rows.into_iter().skip(start).take(end - start).collect();
    Ok(ComponentLinkResult { project_id: project.id, source_cursor, components, total, next_offset: (end < total).then_some(end), unknown_ids })
}
