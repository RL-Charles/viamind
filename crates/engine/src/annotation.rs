//! Bounded whole-schematic reference planning and atomic sheet-guarded application.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use viamind_doc::{ObjectId, Project, Sheet, schematic::validate_sheet};
const MAX_NUMBER: u32 = 999_999_999;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationMode {
    #[default]
    KeepExisting,
    ResetAll,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationOrder {
    #[default]
    XThenY,
    YThenX,
}
const fn first_number() -> u32 {
    1
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AnnotationOptions {
    pub mode: AnnotationMode,
    pub order: AnnotationOrder,
    #[schemars(range(min = 1, max = 999999999))]
    pub start: u32,
    /// Complete flat sheet order; omitted/empty uses UUID order.
    pub sheet_order: Vec<ObjectId>,
}
impl Default for AnnotationOptions {
    fn default() -> Self {
        Self { mode: AnnotationMode::default(), order: AnnotationOrder::default(), start: first_number(), sheet_order: vec![] }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnnotateSchematic {
    pub expected_project: ObjectId,
    pub expected_sheets: BTreeMap<ObjectId, Sheet>,
    pub options: AnnotationOptions,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AnnotationChange {
    pub sheet_id: ObjectId,
    pub symbol_id: ObjectId,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct AnnotationPreview {
    pub source_cursor: EventCursor,
    /// Submit this exact value to annotate_schematic; concurrent schematic edits reject.
    pub command: AnnotateSchematic,
    pub sheet_order: Vec<ObjectId>,
    pub symbols: usize,
    pub changes: Vec<AnnotationChange>,
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_annotation", message.into())
}
fn budget(sheets: &BTreeMap<ObjectId, Sheet>) -> Result<usize, CommandError> {
    if sheets.len() > 100 {
        return Err(invalid("annotation supports at most 100 sheets"));
    }
    let (mut objects, mut symbols, mut text) = (sheets.len(), 0usize, 0usize);
    for sheet in sheets.values() {
        text = text.saturating_add(sheet.name.len());
        if let Some(c) = &sheet.content {
            if !c.packages.is_empty() {
                return Err(invalid("multi-unit package annotation is not supported yet; keep explicit shared package references (T115)"));
            }
            objects = objects.saturating_add(c.object_count()).saturating_add(c.pin_count());
            symbols = symbols.saturating_add(c.symbols.len());
            if objects > 10_000 || symbols > 2000 {
                return Err(invalid("annotation supports at most 2000 symbols and 10000 schematic objects/pins"));
            }
            for s in c.symbols.values() {
                text =
                    text.saturating_add(s.designator.len()).saturating_add(s.value.len()).saturating_add(s.footprint.as_ref().map_or(0, String::len));
                for p in &s.pins {
                    text = text.saturating_add(p.number.len()).saturating_add(p.name.len());
                }
            }
            for l in c.labels.values() {
                text = text.saturating_add(l.name.len());
            }
        }
        if objects > 10_000 || symbols > 2000 || text > 256 * 1024 {
            return Err(invalid("annotation supports at most 2000 symbols,10000 schematic objects/pins and 256 KiB schematic text"));
        }
    }
    Ok(symbols)
}
fn validate(sheets: &BTreeMap<ObjectId, Sheet>, project: ObjectId) -> Result<usize, CommandError> {
    let symbols = budget(sheets)?;
    if serde_json::to_vec(sheets).map_err(|e| invalid(e.to_string()))?.len() > 2 * 1024 * 1024 {
        return Err(invalid("annotation sheet guard exceeds 2 MiB serialized limit"));
    }
    let mut ids = BTreeSet::from([project]);
    for (id, sheet) in sheets {
        if *id != sheet.id {
            return Err(invalid("sheet map key differs from identity"));
        }
        validate_sheet(sheet).map_err(invalid)?;
        let mut insert = |id: ObjectId| {
            if id.as_uuid().is_nil() || !ids.insert(id) { Err(invalid("nil or duplicate schematic identity")) } else { Ok(()) }
        };
        insert(sheet.id)?;
        if let Some(c) = &sheet.content {
            for id in c.wires.keys().chain(c.junctions.keys()).chain(c.labels.keys()) {
                insert(*id)?;
            }
            for s in c.symbols.values() {
                insert(s.id)?;
                for p in &s.pins {
                    insert(p.id)?;
                }
            }
        }
    }
    Ok(symbols)
}
fn reference(text: &str) -> Result<(String, Option<u32>), CommandError> {
    let prefix_len = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    if !(1..=16).contains(&prefix_len) {
        return Err(invalid(format!("unsupported reference {text:?}; use 1..16 ASCII letters followed by a number or?")));
    }
    let prefix = text[..prefix_len].to_ascii_uppercase();
    let suffix = &text[prefix_len..];
    if suffix.is_empty() || suffix == "?" {
        return Ok((prefix, None));
    }
    let number: u32 = if suffix.bytes().all(|b| b.is_ascii_digit()) {
        suffix.parse().map_err(|_| invalid(format!("reference number out of range: {text:?}")))?
    } else {
        return Err(invalid(format!("unsupported reference {text:?}; use a positive decimal number or?")));
    };
    if !(1..=MAX_NUMBER).contains(&number) {
        return Err(invalid(format!("reference number must be 1..999999999: {text:?}")));
    }
    Ok((prefix, Some(number)))
}
struct Plan {
    sheets: BTreeMap<ObjectId, Sheet>,
    order: Vec<ObjectId>,
    changes: Vec<AnnotationChange>,
    symbols: usize,
}
fn plan(sheets: &BTreeMap<ObjectId, Sheet>, options: &AnnotationOptions, project: ObjectId) -> Result<Plan, CommandError> {
    let symbols = validate(sheets, project)?;
    if options.sheet_order.len() > 100 {
        return Err(invalid("sheet_order supports at most 100 sheets"));
    }
    if !(1..=MAX_NUMBER).contains(&options.start) {
        return Err(invalid("start must be 1..999999999"));
    }
    let order = if options.sheet_order.is_empty() { sheets.keys().copied().collect() } else { options.sheet_order.clone() };
    if order.len() != sheets.len() || order.iter().copied().collect::<BTreeSet<_>>() != sheets.keys().copied().collect() {
        return Err(invalid("sheet_order must contain every current sheet exactly once"));
    }
    let mut parsed = BTreeMap::new();
    let mut used: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for (sheet_id, sheet) in sheets {
        if let Some(c) = &sheet.content {
            for s in c.symbols.values() {
                let (prefix, number) = reference(&s.designator).map_err(|e| invalid(format!("symbol {}: {e}", s.id)))?;
                if options.mode == AnnotationMode::KeepExisting
                    && let Some(number) = number
                    && !used.entry(prefix.clone()).or_default().insert(number)
                {
                    return Err(invalid(format!(
                        "ambiguous retained reference {:?}; normalized prefix/number already used; choose reset_all or edit references",
                        s.designator
                    )));
                }
                parsed.insert((*sheet_id, s.id), (prefix, number));
            }
        }
    }
    let mut next = sheets.clone();
    let mut changes = vec![];
    let mut counters = BTreeMap::new();
    for &sheet_id in &order {
        let Some(c) = next.get_mut(&sheet_id).and_then(|s| s.content.as_mut()) else {
            continue;
        };
        let mut ids: Vec<_> = c.symbols.keys().copied().collect();
        ids.sort_by_key(|id| {
            let s = &c.symbols[id];
            match options.order {
                AnnotationOrder::XThenY => (s.position.x, s.position.y, *id),
                AnnotationOrder::YThenX => (s.position.y, s.position.x, *id),
            }
        });
        for id in ids {
            let (prefix, old_number) = &parsed[&(sheet_id, id)];
            if options.mode == AnnotationMode::KeepExisting && old_number.is_some() {
                continue;
            }
            let number = counters.entry(prefix.clone()).or_insert(options.start);
            let occupied = used.entry(prefix.clone()).or_default();
            while *number <= MAX_NUMBER && occupied.contains(number) {
                *number += 1;
            }
            if *number > MAX_NUMBER {
                return Err(invalid(format!("no reference numbers remain for prefix {prefix} at the selected start")));
            }
            occupied.insert(*number);
            let after = format!("{prefix}{number}");
            *number += 1;
            let symbol = c.symbols.get_mut(&id).expect("listed symbol");
            if symbol.designator != after {
                changes.push(AnnotationChange { sheet_id, symbol_id: id, before: symbol.designator.clone(), after: after.clone() });
                symbol.designator = after;
            }
        }
    }
    let mut references = BTreeSet::new();
    for sheet in next.values() {
        validate_sheet(sheet).map_err(invalid)?;
        if let Some(c) = &sheet.content {
            for s in c.symbols.values() {
                if !references.insert(&s.designator) {
                    return Err(invalid("annotation produced duplicate reference"));
                }
            }
        }
    }
    Ok(Plan { sheets: next, order, changes, symbols })
}
impl Command for AnnotateSchematic {
    const NAME: &'static str = "annotate_schematic";
    const DESCRIPTION: &'static str = "Atomically assign native flat single-unit symbol reference designators using the exact project UUID and complete expected sheet map from preview_schematic_annotation. Optional complete sheet order (default UUID), X/Y position order, inclusive start 1..999999999, keep_existing or reset_all. ASCII-letter prefixes 1..16; positive numeric/?/empty suffix. Keep existing numeric aliases must be unique ignoring prefix case/leading zeros. Bounded 100 sheets/2000 symbols/10000 members/256KiB text/2MiB guard. Preserves UUIDs, pins, geometry and board; one sheet-scoped undo transaction. Stale sheets reject. No PCB synchronization/hierarchy/multi-unit annotation.";
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        budget(&self.expected_sheets)?;
        if self.expected_project.as_uuid().is_nil() {
            return Err(invalid("nil project identity"));
        }
        if project.id != self.expected_project || project.schematic.sheets != self.expected_sheets {
            return Err(CommandError::conflict("schematic changed; prepare annotation again"));
        }
        let planned = plan(&self.expected_sheets, &self.options, project.id)?;
        let mut deltas = Vec::new();
        for (id, after) in planned.sheets {
            if self.expected_sheets.get(&id) != Some(&after) {
                deltas.push(Delta::record_sheet(project, id, Some(after)));
            }
        }
        Ok(deltas)
    }
}
impl EngineHandle {
    pub fn preview_schematic_annotation(&self, options: AnnotationOptions) -> Result<AnnotationPreview, CommandError> {
        let (project, cursor, sheets) = self.with_engine(|e| {
            let p = e.project();
            budget(&p.schematic.sheets)?;
            Ok::<_, CommandError>((p.id, e.event_cursor(), p.schematic.sheets.clone()))
        })?;
        if project.as_uuid().is_nil() {
            return Err(invalid("nil project identity"));
        }
        let planned = plan(&sheets, &options, project)?;
        Ok(AnnotationPreview {
            source_cursor: cursor,
            command: AnnotateSchematic { expected_project: project, expected_sheets: sheets, options },
            sheet_order: planned.order,
            symbols: planned.symbols,
            changes: planned.changes,
        })
    }
}
