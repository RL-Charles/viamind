//! Bounded whole-schematic reference planning and atomic sheet-guarded application.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::{BTreeMap, BTreeSet};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{SchematicSymbol, validate_sheet},
};
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
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnnotateSchematic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_cursor: Option<EventCursor>,
    pub expected_project: ObjectId,
    pub expected_sheets: BTreeMap<ObjectId, Sheet>,
    pub options: AnnotationOptions,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnotationWire {
    #[serde(default)]
    expected_cursor: Option<EventCursor>,
    expected_project: ObjectId,
    expected_sheets: BTreeMap<ObjectId, Sheet>,
    options: AnnotationOptions,
}
impl<'de> Deserialize<'de> for AnnotateSchematic {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        crate::annotation_bounds::predecode(&value).map_err(de::Error::custom)?;
        let wire: AnnotationWire = serde_json::from_value(value).map_err(de::Error::custom)?;
        Ok(Self {
            expected_cursor: wire.expected_cursor,
            expected_project: wire.expected_project,
            expected_sheets: wire.expected_sheets,
            options: wire.options,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AnnotationChange {
    pub sheet_id: ObjectId,
    pub symbol_id: ObjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_key: Option<String>,
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
    pub packages: usize,
    pub changes: Vec<AnnotationChange>,
}
pub(super) fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_annotation", message.into())
}
use crate::annotation_bounds::budget;
fn validate(sheets: &BTreeMap<ObjectId, Sheet>, project: ObjectId) -> Result<usize, CommandError> {
    let symbols = budget(sheets)?;
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
            for id in c.packages.keys().chain(c.wires.keys()).chain(c.junctions.keys()).chain(c.labels.keys()) {
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
    packages: usize,
}
struct Owner<'a> {
    id: ObjectId,
    anchor: &'a SchematicSymbol,
    package_id: Option<ObjectId>,
    members: Vec<(ObjectId, Option<&'a str>)>,
}
fn owners(sheet: &Sheet) -> Vec<Owner<'_>> {
    let Some(c) = &sheet.content else { return vec![] };
    let mut result = Vec::new();
    let mut members = BTreeSet::new();
    for group in c.packages.values() {
        let mut units: Vec<_> = group.units.iter().collect();
        units.sort_by_key(|u| &u.key);
        let anchor = &c.symbols[&units[0].symbol_id];
        let ids = units
            .into_iter()
            .map(|u| {
                members.insert(u.symbol_id);
                (u.symbol_id, Some(u.key.as_str()))
            })
            .collect();
        result.push(Owner { id: group.id, anchor, package_id: Some(group.id), members: ids });
    }
    for symbol in c.symbols.values().filter(|s| !members.contains(&s.id)) {
        result.push(Owner { id: symbol.id, anchor: symbol, package_id: None, members: vec![(symbol.id, None)] });
    }
    result
}
fn plan(sheets: &BTreeMap<ObjectId, Sheet>, options: &AnnotationOptions, project: ObjectId) -> Result<Plan, CommandError> {
    let symbols = validate(sheets, project)?;
    if options.sheet_order.len() > 100 {
        return Err(invalid("sheet_order supports at most 100 sheets"));
    }
    if !(1..=MAX_NUMBER).contains(&options.start) {
        return Err(invalid("start must be 1..999999999"));
    }
    let order: Vec<_> = if options.sheet_order.is_empty() { sheets.keys().copied().collect() } else { options.sheet_order.clone() };
    if order.len() != sheets.len() || order.iter().copied().collect::<BTreeSet<_>>() != sheets.keys().copied().collect() {
        return Err(invalid("sheet_order must contain every current sheet exactly once"));
    }
    let owners: BTreeMap<_, _> = sheets.iter().map(|(id, s)| (*id, owners(s))).collect();
    let packages = owners.values().flatten().filter(|o| o.package_id.is_some()).count();
    let mut parsed = BTreeMap::new();
    let mut used: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for (sheet_id, owners) in &owners {
        for owner in owners {
            let (prefix, number) = reference(&owner.anchor.designator).map_err(|e| invalid(format!("reference owner {}: {e}", owner.id)))?;
            if options.mode == AnnotationMode::KeepExisting
                && let Some(number) = number
                && !used.entry(prefix.clone()).or_default().insert(number)
            {
                return Err(invalid(format!(
                    "ambiguous retained reference {:?}; normalized prefix/number already used; choose reset_all or edit references",
                    owner.anchor.designator
                )));
            }
            parsed.insert((*sheet_id, owner.id), (prefix, number));
        }
    }
    let mut next = sheets.clone();
    let mut changes = vec![];
    let mut counters = BTreeMap::new();
    let mut final_refs = BTreeSet::new();
    for &sheet_id in &order {
        let mut ordered: Vec<_> = owners[&sheet_id].iter().collect();
        ordered.sort_by_key(|o| match options.order {
            AnnotationOrder::XThenY => (o.anchor.position.x, o.anchor.position.y, o.id),
            AnnotationOrder::YThenX => (o.anchor.position.y, o.anchor.position.x, o.id),
        });
        for owner in ordered {
            let (prefix, old_number) = &parsed[&(sheet_id, owner.id)];
            let after = if options.mode == AnnotationMode::KeepExisting && old_number.is_some() {
                owner.anchor.designator.clone()
            } else {
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
                after
            };
            if !final_refs.insert(after.clone()) {
                return Err(invalid("annotation produced duplicate owner reference"));
            }
            let c = next.get_mut(&sheet_id).and_then(|s| s.content.as_mut()).expect("owner sheet content");
            for &(id, key) in &owner.members {
                let symbol = c.symbols.get_mut(&id).expect("validated member");
                if symbol.designator != after {
                    changes.push(AnnotationChange {
                        sheet_id,
                        symbol_id: id,
                        package_id: owner.package_id,
                        unit_key: key.map(str::to_owned),
                        before: symbol.designator.clone(),
                        after: after.clone(),
                    });
                    symbol.designator = after.clone();
                }
            }
        }
    }
    for sheet in next.values() {
        validate_sheet(sheet).map_err(invalid)?;
    }
    Ok(Plan { sheets: next, order, changes, symbols, packages })
}
impl Command for AnnotateSchematic {
    const NAME: &'static str = "annotate_schematic";
    const DESCRIPTION: &'static str = "Atomically assign native flat single-unit and explicit whole-package reference designators using the exact project UUID and complete expected sheet map from preview_schematic_annotation. Optional complete sheet order (default UUID), X/Y position order, inclusive start 1..999999999, keep_existing or reset_all. ASCII-letter prefixes 1..16; positive numeric/?/empty suffix. Keep existing numeric aliases must be unique ignoring prefix case/leading zeros. Bounded 100 sheets/2000 symbols/10000 members/256KiB text/2MiB guard. Preserves UUIDs, pins, geometry and board; one sheet-scoped undo transaction. New previews include a full cursor; packages require it. Stale cursor or sheets reject. One reference per explicit package owner; all member units change atomically. No PCB synchronization/hierarchy.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        self.expected_cursor
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        budget(&self.expected_sheets)?;
        crate::annotation_bounds::envelope(&self)?;
        if self.expected_cursor.is_none() && self.expected_sheets.values().any(|s| s.content.as_ref().is_some_and(|c| !c.packages.is_empty())) {
            return Err(invalid("package annotation requires expected_cursor from preview_schematic_annotation"));
        }
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
            #[derive(Serialize)]
            struct Guard<'a> {
                expected_cursor: EventCursor,
                expected_project: ObjectId,
                expected_sheets: &'a BTreeMap<ObjectId, Sheet>,
                options: &'a AnnotationOptions,
            }
            crate::annotation_bounds::envelope(&Guard {
                expected_cursor: e.event_cursor(),
                expected_project: p.id,
                expected_sheets: &p.schematic.sheets,
                options: &options,
            })?;
            Ok::<_, CommandError>((p.id, e.event_cursor(), p.schematic.sheets.clone()))
        })?;
        if project.as_uuid().is_nil() {
            return Err(invalid("nil project identity"));
        }
        let planned = plan(&sheets, &options, project)?;
        Ok(AnnotationPreview {
            source_cursor: cursor,
            command: AnnotateSchematic { expected_cursor: Some(cursor), expected_project: project, expected_sheets: sheets, options },
            sheet_order: planned.order,
            symbols: planned.symbols,
            packages: planned.packages,
            changes: planned.changes,
        })
    }
}
