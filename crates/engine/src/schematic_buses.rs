//! Explicit nonconducting buses, coherent scalar facts and guarded member unfolding.
use crate::{Command, CommandError, Delta, EngineHandle, EventCursor, SchematicConnectivityQuery, SchematicConnectivityResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::atomic::{AtomicUsize, Ordering},
};
use viamind_doc::{
    ObjectId, Project, SchematicBus, SchematicBusEntry, Sheet,
    schematic::{Coordinate, LabelScope, SchematicLabel, SchematicWire},
};
const MAX_VISITS: usize = 1_000_000;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
fn invalid(s: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_buses", s)
}
struct Permit;
impl Permit {
    fn reserve() -> Result<Self, CommandError> {
        let mut n = ACTIVE.load(Ordering::Acquire);
        loop {
            if n >= 2 {
                return Err(invalid("two bus query/planning workers are busy; retry"));
            }
            match ACTIVE.compare_exchange_weak(n, n + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(Self),
                Err(next) => n = next,
            }
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}
fn charge(remaining: &mut usize) -> Result<(), CommandError> {
    *remaining = remaining.checked_sub(1).ok_or_else(|| invalid("bus proof exceeds shared1000000 candidate/graph visits"))?;
    Ok(())
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BusSource {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub sheet_id: ObjectId,
}
impl BusSource {
    fn check<'a>(&self, p: &'a Project) -> Result<&'a Sheet, CommandError> {
        if p.id != self.expected_project {
            return Err(CommandError::conflict("project changed; reload buses"));
        }
        p.schematic.sheets.get(&self.sheet_id).ok_or_else(|| CommandError::not_found("sheet", self.sheet_id))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSchematicBus {
    pub source: BusSource,
    pub bus_id: Option<ObjectId>,
    pub expression: String,
    pub start: Coordinate,
    pub end: Coordinate,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteSchematicBus {
    pub source: BusSource,
    pub bus_id: ObjectId,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteSchematicBusBreakout {
    pub source: BusSource,
    pub entry_id: ObjectId,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnfoldSchematicBusMember {
    pub source: BusSource,
    pub bus_id: ObjectId,
    pub member: String,
    pub bus_point: Coordinate,
    pub wire_point: Coordinate,
    pub wire_end: Coordinate,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicBusesQuery {}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScalarBusLabelFact {
    pub label_id: ObjectId,
    pub scope: LabelScope,
    pub net_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScalarBusMemberFact {
    pub name: String,
    pub labels: Vec<ScalarBusLabelFact>,
    pub missing: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchematicBusFact {
    pub sheet_id: ObjectId,
    pub bus: SchematicBus,
    pub entries: Vec<SchematicBusEntry>,
    pub members: Vec<ScalarBusMemberFact>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchematicBusCoverage {
    pub sheets: usize,
    pub symbols: usize,
    pub symbols_without_pins: usize,
    pub scalar_members: usize,
    pub scalar_nets: usize,
    pub max_graph_rows: usize,
    pub max_text_bytes: usize,
    pub max_visits: usize,
    pub max_output_bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchematicBusesResult {
    pub coverage: SchematicBusCoverage,
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub complete: bool,
    pub buses: Vec<SchematicBusFact>,
    pub total_buses: usize,
    pub total_entries: usize,
    pub total_expanded_members: usize,
    pub legacy_empty_sheets: usize,
    pub exact_candidate_visits: usize,
    pub exclusions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchematicBusUnfoldPreview {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub sheet_id: ObjectId,
    pub bus_id: ObjectId,
    pub member: String,
    pub bus_point: Coordinate,
    pub wire_point: Coordinate,
    pub wire_end: Coordinate,
    pub allowed_original_component_count: usize,
    pub original_graph_members: usize,
    pub exact_candidate_visits: usize,
    pub unrelated_partitions_preserved: bool,
    pub scope: String,
}
struct Counter {
    used: usize,
}
impl Counter {
    fn charge(&mut self, n: usize) -> Result<(), CommandError> {
        self.used = self.used.checked_add(n).ok_or_else(|| invalid("bus output size overflow"))?;
        if self.used > MAX_OUTPUT {
            return Err(invalid("complete bus query exceeds2MiB output budget"));
        }
        Ok(())
    }
}
struct Writer {
    bytes: Vec<u8>,
}
impl Write for Writer {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() > MAX_OUTPUT.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("bus output exceeds2MiB"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn serialize(value: &impl Serialize) -> Result<Vec<u8>, CommandError> {
    let mut w = Writer { bytes: vec![] };
    serde_json::to_writer(&mut w, value).map_err(|e| invalid(e.to_string()))?;
    Ok(w.bytes)
}
fn fresh(ids: &mut BTreeSet<ObjectId>, mut generate: impl FnMut() -> ObjectId) -> Result<ObjectId, CommandError> {
    for _ in 0..1024 {
        let id = generate();
        if !id.as_uuid().is_nil() && ids.insert(id) {
            return Ok(id);
        }
    }
    Err(invalid("fresh bus identity exhausted1024 attempts; unchanged"))
}
fn global_ids(p: &Project) -> BTreeSet<ObjectId> {
    let mut ids = BTreeSet::from([p.id]);
    for part in p.board.parts.values() {
        ids.insert(part.id);
        ids.extend(part.pads.iter().flat_map(|p| &p.0).map(|p| p.id));
        ids.extend(part.holes.iter().flatten().map(|h| h.id));
    }
    ids.extend(p.board.tracks.keys().chain(p.board.vias.keys()).copied());
    for s in p.schematic.sheets.values() {
        ids.insert(s.id);
        if let Some(c) = &s.content {
            ids.extend(
                c.buses
                    .keys()
                    .chain(c.bus_entries.keys())
                    .chain(c.packages.keys())
                    .chain(c.symbols.keys())
                    .chain(c.wires.keys())
                    .chain(c.junctions.keys())
                    .chain(c.labels.keys())
                    .copied(),
            );
            ids.extend(c.symbols.values().flat_map(|s| &s.pins).map(|p| p.id));
        }
    }
    ids
}
fn admit(p: &Project, source: BusSource, added: usize) -> Result<&Sheet, CommandError> {
    let s = source.check(p)?;
    crate::schematic_budget::project(p, added, "bus operation")?;
    crate::schematic_budget::sheet(s, "bus operation")?;
    viamind_doc::schematic::validate_project(p, None).map_err(invalid)?;
    Ok(s)
}
fn record(p: &mut Project, s: Sheet) -> Result<Vec<Delta>, CommandError> {
    viamind_doc::schematic::validate_project(p, Some(&s)).map_err(invalid)?;
    if p.schematic.sheets.get(&s.id) == Some(&s) {
        return Ok(vec![]);
    }
    Ok(vec![Delta::record_sheet(p, s.id, Some(s))])
}
impl Command for SetSchematicBus {
    const NAME: &'static str = "set_schematic_bus";
    const DESCRIPTION: &'static str = "Create or edit one explicit nonconducting schematic bus. Exact project/full event cursor/sheet guards; omit bus_id to create. Strict bounded unsigned vectors NAME[M..N] or flat optional-prefix groups NAME{TERM TERM}. Integer Nm cardinal segment; no implicit scalar label interpretation/conductance. Existing breakout associations must remain valid. Maximum256 project buses/4096 entries/10000 expanded members/2MiB bus text;1024 fresh-ID attempts. One undoable sheet delta, unchanged on refusal.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source.source_cursor)
    }
    fn apply(self, p: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let new = viamind_doc::schematic_bus::expression_budget(&self.expression).map_err(invalid)?;
        self.start.validate().map_err(invalid)?;
        self.end.validate().map_err(invalid)?;
        if self.start == self.end || (self.start.x != self.end.x && self.start.y != self.end.y) {
            return Err(invalid("bus must be a nonzero cardinal segment"));
        }
        let s = admit(p, self.source, usize::from(self.bus_id.is_none()))?;
        let total = viamind_doc::schematic_bus::preflight_project(p, None).map_err(invalid)?;
        let old =
            self.bus_id.map(|id| s.content.as_ref().and_then(|c| c.buses.get(&id)).ok_or_else(|| CommandError::not_found("bus", id))).transpose()?;
        let old_budget = old.map(|b| viamind_doc::schematic_bus::expression_budget(&b.expression)).transpose().map_err(invalid)?.unwrap_or_default();
        if total.buses + usize::from(old.is_none()) > 256
            || total.members - old_budget.members + new.members > 10000
            || total.text_bytes - old.map_or(0, |b| b.expression.len()) - old_budget.text_bytes + self.expression.len() + new.text_bytes
                > 2 * 1024 * 1024
        {
            return Err(invalid("projected bus count/member/text budget exceeded before copy"));
        }
        let members = viamind_doc::schematic_bus::expand(&self.expression).map_err(invalid)?;
        if let (Some(c), Some(id)) = (&s.content, self.bus_id) {
            for e in c.bus_entries.values().filter(|e| e.bus_id == id) {
                if !members.contains(&e.member) || !on(e.bus_point, self.start, self.end) {
                    return Err(invalid("edit would invalidate an owned breakout; delete it first"));
                }
            }
        }
        let mut sheet = s.clone();
        let mut ids = global_ids(p);
        let id = match self.bus_id {
            Some(id) => id,
            None => fresh(&mut ids, ObjectId::new)?,
        };
        sheet
            .content
            .get_or_insert_with(Default::default)
            .buses
            .insert(id, SchematicBus { id, expression: self.expression, start: self.start, end: self.end });
        record(p, sheet)
    }
}
impl Command for DeleteSchematicBus {
    const NAME: &'static str = "delete_schematic_bus";
    const DESCRIPTION: &'static str = "Delete one explicit schematic bus guarded by project UUID/full event cursor. Refuses a bus with owned entries; delete each complete breakout first. Does not delete or rename other scalar wires/labels. One sheet delta/undo; bounded preflight before copying.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source.source_cursor)
    }
    fn apply(self, p: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let s = admit(p, self.source, 0)?;
        let c = s.content.as_ref().ok_or_else(|| CommandError::not_found("bus", self.bus_id))?;
        if !c.buses.contains_key(&self.bus_id) {
            return Err(CommandError::not_found("bus", self.bus_id));
        }
        if c.bus_entries.values().any(|e| e.bus_id == self.bus_id) {
            return Err(invalid("delete owned breakouts before deleting their bus"));
        }
        let mut s = s.clone();
        s.content.as_mut().expect("checked content").buses.remove(&self.bus_id);
        record(p, s)
    }
}
impl Command for DeleteSchematicBusBreakout {
    const NAME: &'static str = "delete_schematic_bus_breakout";
    const DESCRIPTION: &'static str = "Delete exactly one explicit entry and its associated scalar wire and Local member label, guarded by project UUID/full event cursor. Keeps the parent bus and every other object. One undoable sheet delta; native association validation and borrowed limits precede copy.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source.source_cursor)
    }
    fn apply(self, p: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let s = admit(p, self.source, 0)?;
        let e =
            s.content.as_ref().and_then(|c| c.bus_entries.get(&self.entry_id)).ok_or_else(|| CommandError::not_found("bus entry", self.entry_id))?;
        let (wire, label) = (e.wire_id, e.label_id);
        let mut s = s.clone();
        let c = s.content.as_mut().expect("checked content");
        c.bus_entries.remove(&self.entry_id);
        c.wires.remove(&wire);
        c.labels.remove(&label);
        record(p, s)
    }
}
fn on(p: Coordinate, a: Coordinate, b: Coordinate) -> bool {
    let (x, y, ax, ay, bx, by) = (i128::from(p.x.0), i128::from(p.y.0), i128::from(a.x.0), i128::from(a.y.0), i128::from(b.x.0), i128::from(b.y.0));
    (x - ax) * (by - ay) == (y - ay) * (bx - ax) && x >= ax.min(bx) && x <= ax.max(bx) && y >= ay.min(by) && y <= ay.max(by)
}
enum Request {
    Query,
    Preview(UnfoldSchematicBusMember),
}
pub struct SchematicBusWork {
    engine: EngineHandle,
    request: Request,
    _permit: Permit,
}
impl EngineHandle {
    pub fn prepare_schematic_buses(&self, _: &SchematicBusesQuery) -> Result<SchematicBusWork, CommandError> {
        crate::command_hooks::check_read().map_err(|e| invalid(e.to_string()))?;
        Ok(SchematicBusWork { engine: self.clone(), request: Request::Query, _permit: Permit::reserve()? })
    }
    pub fn prepare_schematic_bus_unfold(&self, input: UnfoldSchematicBusMember) -> Result<SchematicBusWork, CommandError> {
        crate::command_hooks::check_read().map_err(|e| invalid(e.to_string()))?;
        input.check_small()?;
        self.with_engine(|e| {
            if e.event_cursor() != input.source.source_cursor {
                return Err(CommandError::conflict("source cursor changed; reload bus preview"));
            }
            input.source.check(e.project()).map(|_| ())
        })?;
        Ok(SchematicBusWork { engine: self.clone(), request: Request::Preview(input), _permit: Permit::reserve()? })
    }
}
impl SchematicBusWork {
    pub fn run_value(self) -> Result<serde_json::Value, CommandError> {
        let bytes = self.execute()?;
        serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
    }
    pub fn run_query(self) -> Result<SchematicBusesResult, CommandError> {
        let bytes = self.execute()?;
        serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
    }
    pub fn run_preview(self) -> Result<SchematicBusUnfoldPreview, CommandError> {
        let bytes = self.execute()?;
        serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
    }
    fn execute(&self) -> Result<Vec<u8>, CommandError> {
        crate::command_hooks::check_read().map_err(|e| invalid(e.to_string()))?;
        match &self.request {
            Request::Query => {
                let snap = self.engine.with_engine(|e| {
                    crate::schematic_budget::project(e.project(), 0, "bus query")?;
                    crate::schematic_connectivity::capture(e.project(), e.event_cursor())
                })?;
                serialize(&query(snap)?)
            }
            Request::Preview(input) => {
                let (snap, ids) = self.engine.with_engine(|e| {
                    if e.event_cursor() != input.source.source_cursor {
                        return Err(CommandError::conflict("source cursor changed; reload bus preview"));
                    }
                    capture_unfold(e.project(), input)
                })?;
                serialize(&plan(snap, ids, input)?.preview)
            }
        }
    }
}
fn query(snapshot: crate::schematic_connectivity::Snapshot) -> Result<SchematicBusesResult, CommandError> {
    let mut remaining = MAX_VISITS;
    let graph = crate::schematic_connectivity::calculate_with_budget(
        snapshot.clone(),
        &SchematicConnectivityQuery { offset: 0, limit: 10000 },
        &mut remaining,
    )?;
    if graph.next_offset.is_some() {
        return Err(invalid("whole scalar graph unexpectedly truncated"));
    }
    let mut index: BTreeMap<(ObjectId, &str), Vec<(&crate::SchematicNetName, &str)>> = BTreeMap::new();
    for net in &graph.nets {
        for name in &net.names {
            charge(&mut remaining)?;
            index.entry((name.sheet_id, &name.name)).or_default().push((name, &net.id));
        }
    }
    let mut out = Counter { used: 512 };
    let mut buses = Vec::new();
    let mut total_entries = 0;
    let mut total_expanded_members = 0;
    for s in &snapshot.sheets {
        let Some(c) = &s.content else { continue };
        for b in c.buses.values() {
            charge(&mut remaining)?;
            out.charge(512 + b.expression.len() * 6)?;
            let names = viamind_doc::schematic_bus::expand(&b.expression).map_err(invalid)?;
            let mut members = Vec::with_capacity(names.len());
            for name in names {
                charge(&mut remaining)?;
                out.charge(128 + name.len() * 6)?;
                let mut labels = Vec::new();
                if let Some(facts) = index.get(&(s.id, name.as_str())) {
                    for (n, net_id) in facts {
                        charge(&mut remaining)?;
                        out.charge(160 + net_id.len() * 6)?;
                        labels.push(ScalarBusLabelFact { label_id: n.label_id, scope: n.scope, net_id: (*net_id).into() });
                    }
                }
                total_expanded_members += 1;
                members.push(ScalarBusMemberFact { name, missing: labels.is_empty(), labels });
            }
            let mut entries = Vec::new();
            for e in c.bus_entries.values() {
                charge(&mut remaining)?;
                if e.bus_id == b.id {
                    out.charge(1024 + e.member.len() * 6)?;
                    entries.push(e.clone());
                    total_entries += 1;
                }
            }
            buses.push(SchematicBusFact { sheet_id: s.id, bus: b.clone(), entries, members });
        }
    }
    let total_buses = buses.len();
    Ok(SchematicBusesResult {
        coverage: SchematicBusCoverage {
            sheets: graph.coverage.sheets,
            symbols: graph.coverage.symbols,
            symbols_without_pins: graph.coverage.symbols_without_pins,
            scalar_members: graph.coverage.members,
            scalar_nets: graph.total_nets,
            max_graph_rows: 10000,
            max_text_bytes: 2 * 1024 * 1024,
            max_visits: MAX_VISITS,
            max_output_bytes: MAX_OUTPUT,
        },
        project_id: snapshot.project_id,
        source_cursor: snapshot.source_cursor,
        complete: true,
        buses,
        total_buses,
        total_entries,
        total_expanded_members,
        legacy_empty_sheets: snapshot.sheets.iter().filter(|s| s.content.is_none()).count(),
        exact_candidate_visits: MAX_VISITS - remaining,
        exclusions: vec![
            "Bus and entry graphics do not conduct; facts identify actual exact-name scalar labels on the declaring sheet".into(),
            "No hierarchical buses, aliases, cross-sheet member inference, ERC or PCB continuity proof".into(),
        ],
    })
}
impl UnfoldSchematicBusMember {
    fn check_small(&self) -> Result<(), CommandError> {
        if self.member.is_empty() || self.member.len() > 512 || self.member.chars().count() > 128 || self.member.chars().any(char::is_control) {
            return Err(invalid("member requires1..128 characters/512bytes without controls"));
        }
        self.bus_point.validate().map_err(invalid)?;
        self.wire_point.validate().map_err(invalid)?;
        self.wire_end.validate().map_err(invalid)?;
        let dx = i128::from(self.bus_point.x.0) - i128::from(self.wire_point.x.0);
        let dy = i128::from(self.bus_point.y.0) - i128::from(self.wire_point.y.0);
        if dx == 0 || dx.abs() != dy.abs() {
            return Err(invalid("entry must be a nonzero45degree diagonal"));
        }
        if self.wire_point == self.wire_end || (self.wire_point.x != self.wire_end.x && self.wire_point.y != self.wire_end.y) {
            return Err(invalid("scalar wire must be a nonzero cardinal segment"));
        }
        Ok(())
    }
}
fn capture_unfold(
    p: &Project,
    input: &UnfoldSchematicBusMember,
) -> Result<(crate::schematic_connectivity::Snapshot, BTreeSet<ObjectId>), CommandError> {
    input.check_small()?;
    let s = admit(p, input.source, 3)?;
    let c = s.content.as_ref().ok_or_else(|| CommandError::not_found("bus", input.bus_id))?;
    let b = c.buses.get(&input.bus_id).ok_or_else(|| CommandError::not_found("bus", input.bus_id))?;
    if !on(input.bus_point, b.start, b.end) || !viamind_doc::schematic_bus::expand(&b.expression).map_err(invalid)?.contains(&input.member) {
        return Err(invalid("choose an exact declared member and a point on its bus"));
    }
    let totals = viamind_doc::schematic_bus::preflight_project(p, None).map_err(invalid)?;
    if totals.entries >= 4096 || totals.text_bytes.saturating_add(input.member.len()) > 2 * 1024 * 1024 || c.object_count().saturating_add(3) > 10000
    {
        return Err(invalid("projected entry/text/sheet limit exceeded before copy"));
    }
    let mut rows = 3usize;
    let mut text = input.member.len() * 2;
    for sheet in p.schematic.sheets.values() {
        text = text.saturating_add(crate::schematic_budget::sheet(sheet, "unfold capture")?);
        let count = if let Some(c) = &sheet.content {
            let (membership, _) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            c.object_count().saturating_add(c.pin_count()).saturating_add(membership.saturating_sub(c.packages.len()))
        } else {
            0
        };
        rows = rows.saturating_add(1).saturating_add(count);
        if rows > 10000 || text > 2 * 1024 * 1024 {
            return Err(invalid("projected graph exceeds10000 rows or2MiB before copy"));
        }
    }
    let snapshot = crate::schematic_connectivity::capture(p, input.source.source_cursor)?;
    Ok((snapshot, global_ids(p)))
}
struct Plan {
    sheet: Sheet,
    preview: SchematicBusUnfoldPreview,
    new_ids: [ObjectId; 3],
    ids: BTreeSet<ObjectId>,
}
fn plan(
    snapshot: crate::schematic_connectivity::Snapshot,
    mut ids: BTreeSet<ObjectId>,
    input: &UnfoldSchematicBusMember,
) -> Result<Plan, CommandError> {
    let mut next = 1u128;
    let mut temporary = || {
        let id = format!("{next:032x}").parse::<ObjectId>().expect("fixed32hex identity");
        next += 1;
        id
    };
    let new_ids = [fresh(&mut ids, &mut temporary)?, fresh(&mut ids, &mut temporary)?, fresh(&mut ids, &mut temporary)?];
    let [entry, wire, label] = new_ids;
    let mut candidate = snapshot.clone();
    let s = candidate.sheets.iter_mut().find(|s| s.id == input.source.sheet_id).ok_or_else(|| invalid("selected sheet missing in captured graph"))?;
    let c = s.content.as_mut().expect("checked bus content");
    c.wires.insert(wire, SchematicWire { id: wire, start: input.wire_point, end: input.wire_end });
    c.labels.insert(label, SchematicLabel { id: label, position: input.wire_point, name: input.member.clone(), scope: LabelScope::Local });
    c.bus_entries.insert(
        entry,
        SchematicBusEntry {
            id: entry,
            bus_id: input.bus_id,
            member: input.member.clone(),
            wire_id: wire,
            label_id: label,
            bus_point: input.bus_point,
            wire_point: input.wire_point,
        },
    );
    crate::schematic_budget::sheet(s, "unfold candidate")?;
    let sheet = s.clone();
    let source = snapshot.sheets.iter().find(|s| s.id == input.source.sheet_id).expect("source sheet");
    let mut remaining = MAX_VISITS;
    let mut target_wires = BTreeSet::new();
    for w in source.content.as_ref().expect("source content").wires.values() {
        charge(&mut remaining)?;
        if on(input.wire_end, w.start, w.end) {
            target_wires.insert(w.id);
        }
    }
    let query = SchematicConnectivityQuery { offset: 0, limit: 10000 };
    let before = crate::schematic_connectivity::calculate_with_budget(snapshot, &query, &mut remaining)?;
    let after = crate::schematic_connectivity::calculate_with_budget(candidate, &query, &mut remaining)?;
    let allowed = preserve(&before, &after, &target_wires, input, wire, label, &mut remaining)?;
    Ok(Plan{sheet,preview:SchematicBusUnfoldPreview{expected_project:input.source.expected_project,source_cursor:input.source.source_cursor,sheet_id:input.source.sheet_id,bus_id:input.bus_id,member:input.member.clone(),bus_point:input.bus_point,wire_point:input.wire_point,wire_end:input.wire_end,allowed_original_component_count:allowed,original_graph_members:before.coverage.members,exact_candidate_visits:MAX_VISITS-remaining,unrelated_partitions_preserved:true,scope:"Actual original scalar UUID partitions and label scopes preserved except the explicit endpoint/same-sheet Local-member union; bus/entry graphics do not conduct. No ERC/PCB/certification proof".into()},new_ids,ids})
}
fn preserve(
    before: &SchematicConnectivityResult,
    after: &SchematicConnectivityResult,
    target_wires: &BTreeSet<ObjectId>,
    input: &UnfoldSchematicBusMember,
    wire: ObjectId,
    label: ObjectId,
    remaining: &mut usize,
) -> Result<usize, CommandError> {
    if before.next_offset.is_some() || after.next_offset.is_some() {
        return Err(invalid("complete graph proof unexpectedly truncated"));
    }
    let mut originals = BTreeSet::new();
    let mut expected = BTreeSet::new();
    let mut union = BTreeSet::new();
    let mut allowed = 0usize;
    let mut old_names = BTreeMap::new();
    for net in &before.nets {
        let mut group = BTreeSet::new();
        let mut target = false;
        for m in &net.members {
            charge(remaining)?;
            group.insert(m.id);
            originals.insert(m.id);
            if m.sheet_id == input.source.sheet_id && (target_wires.contains(&m.id) || m.position == Some(input.wire_end)) {
                target = true;
            }
        }
        for n in &net.names {
            charge(remaining)?;
            old_names.insert(n.label_id, (n.sheet_id, n.name.as_str(), n.scope));
            if n.sheet_id == input.source.sheet_id && n.scope == LabelScope::Local && n.name == input.member {
                target = true;
            }
        }
        if target {
            union.extend(group);
            allowed += 1;
        } else {
            expected.insert(group);
        }
    }
    if !union.is_empty() {
        expected.insert(union.clone());
    }
    let mut actual = BTreeSet::new();
    let mut found = BTreeSet::new();
    let mut new_group = None;
    let mut found_names = BTreeMap::new();
    for net in &after.nets {
        let mut group = BTreeSet::new();
        let mut has_wire = false;
        let mut has_label = false;
        for m in &net.members {
            charge(remaining)?;
            if originals.contains(&m.id) {
                group.insert(m.id);
                found.insert(m.id);
            }
            has_wire |= m.id == wire;
            has_label |= m.id == label;
        }
        for n in &net.names {
            charge(remaining)?;
            if n.label_id != label {
                found_names.insert(n.label_id, (n.sheet_id, n.name.as_str(), n.scope));
            }
        }
        if has_wire || has_label {
            if !has_wire || !has_label || new_group.is_some() {
                return Err(invalid("new scalar wire/label did not form one exact component"));
            }
            new_group = Some(group.clone());
        }
        if !group.is_empty() && !actual.insert(group) {
            return Err(invalid("candidate split an original scalar component"));
        }
    }
    if actual != expected || found != originals || old_names != found_names || new_group.as_ref() != Some(&union) {
        return Err(invalid("unfold would alter unrelated original scalar partitions or label scopes; move the explicit wire"));
    }
    Ok(allowed)
}
impl Command for UnfoldSchematicBusMember {
    const NAME: &'static str = "unfold_schematic_bus_member";
    const DESCRIPTION: &'static str = "Atomically unfold one exact member of an explicit bus into a nonconducting45degree entry plus ordinary cardinal wire and Local member label. Requires project/full source cursor; preview_schematic_bus_unfold first. Actual production source/candidate scalar graph permits only the explicit opposite-wire-endpoint and same-sheet exact Local-member union; refuses incidental shorts or any other original UUID partition/name/scope change. Cumulative1M visits/10000 graph rows/2MiB capture, two retained planners,1024 fresh-ID attempts. One sheet delta/undo; no global-only name inference, ERC or PCB certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source.source_cursor)
    }
    fn apply(self, p: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let _permit = Permit::reserve()?;
        let (snap, ids) = capture_unfold(p, &self)?;
        let mut plan = plan(snap, ids, &self)?;
        let [old_entry, old_wire, old_label] = plan.new_ids;
        let entry = fresh(&mut plan.ids, ObjectId::new)?;
        let wire = fresh(&mut plan.ids, ObjectId::new)?;
        let label = fresh(&mut plan.ids, ObjectId::new)?;
        let c = plan.sheet.content.as_mut().expect("planned content");
        let mut w = c.wires.remove(&old_wire).expect("planned wire");
        w.id = wire;
        c.wires.insert(wire, w);
        let mut l = c.labels.remove(&old_label).expect("planned label");
        l.id = label;
        c.labels.insert(label, l);
        let mut e = c.bus_entries.remove(&old_entry).expect("planned entry");
        e.id = entry;
        e.wire_id = wire;
        e.label_id = label;
        c.bus_entries.insert(entry, e);
        record(p, plan.sheet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineHandle, Origin};
    use std::sync::Mutex;
    use viamind_geom::Nm;
    static LOCK: Mutex<()> = Mutex::new(());
    fn p(x: i64, y: i64) -> Coordinate {
        Coordinate { x: Nm(x), y: Nm(y) }
    }
    fn fixture() -> (EngineHandle, ObjectId, ObjectId) {
        let mut project = Project::new("bus oracle");
        let mut s = Sheet::new("Buses");
        let b = SchematicBus { id: ObjectId::new(), expression: "DATA[1..0]".into(), start: p(0, 0), end: p(10, 0) };
        let id = b.id;
        let sid = s.id;
        let mut c = viamind_doc::schematic::SchematicContent::default();
        c.buses.insert(id, b);
        s.content = Some(c);
        project.schematic.sheets.insert(sid, s);
        viamind_doc::schematic::validate_project(&project, None).unwrap();
        (EngineHandle::new(project), sid, id)
    }
    fn source(e: &EngineHandle, sheet: ObjectId) -> BusSource {
        BusSource { expected_project: e.read(|p| p.id), source_cursor: e.event_cursor(), sheet_id: sheet }
    }
    fn unfold(e: &EngineHandle, sheet: ObjectId, bus: ObjectId) -> UnfoldSchematicBusMember {
        UnfoldSchematicBusMember {
            source: source(e, sheet),
            bus_id: bus,
            member: "DATA0".into(),
            bus_point: p(0, 0),
            wire_point: p(4, 4),
            wire_end: p(4, 8),
        }
    }
    fn add_wire(pj: &mut Project, sheet: ObjectId, a: Coordinate, b: Coordinate) -> ObjectId {
        let id = ObjectId::new();
        pj.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap().wires.insert(id, SchematicWire { id, start: a, end: b });
        id
    }
    fn add_label(pj: &mut Project, sheet: ObjectId, point: Coordinate, name: &str, scope: LabelScope) -> ObjectId {
        let id = ObjectId::new();
        pj.schematic
            .sheets
            .get_mut(&sheet)
            .unwrap()
            .content
            .as_mut()
            .unwrap()
            .labels
            .insert(id, SchematicLabel { id, position: point, name: name.into(), scope });
        id
    }
    #[test]
    fn graphical_bus_crossings_do_not_conduct_and_old_bracket_labels_stay_literal() {
        let _guard = LOCK.lock().unwrap();
        let (e, sheet, bus) = fixture();
        let mut pj = e.read(Clone::clone);
        add_wire(&mut pj, sheet, p(-2, 0), p(2, 0));
        add_wire(&mut pj, sheet, p(5, -2), p(5, 2));
        add_label(&mut pj, sheet, p(40, 40), "DATA[0..1]", LabelScope::Local);
        let e = EngineHandle::new(pj);
        let before = e.session_state();
        let r = e.prepare_schematic_buses(&SchematicBusesQuery {}).unwrap().run_query().unwrap();
        assert!(r.complete);
        assert_eq!(r.total_buses, 1);
        assert_eq!(r.total_expanded_members, 2);
        assert!(r.buses[0].members.iter().all(|m| m.missing));
        let graph = e.all_schematic_connectivity().unwrap();
        assert_eq!(graph.total_nets, 3);
        assert!(graph.nets.iter().any(|n| n.canonical_name.as_deref() == Some("DATA[0..1]")));
        assert_eq!(e.session_state(), before);
        assert_eq!(r.buses[0].bus.id, bus);
    }
    #[test]
    fn actual_local_scalar_union_preview_commit_and_origin_undo() {
        let _guard = LOCK.lock().unwrap();
        let (e, sheet, bus) = fixture();
        let mut pj = e.read(Clone::clone);
        let wire = add_wire(&mut pj, sheet, p(4, 8), p(8, 8));
        let label = add_label(&mut pj, sheet, p(100, 100), "DATA0", LabelScope::Local);
        let untouched = add_label(&mut pj, sheet, p(200, 200), "OTHER", LabelScope::Local);
        let e = EngineHandle::new(pj);
        let before = e.session_state();
        let input = unfold(&e, sheet, bus);
        let preview = e.prepare_schematic_bus_unfold(input.clone()).unwrap().run_preview().unwrap();
        assert_eq!(preview.allowed_original_component_count, 2);
        assert_eq!(e.session_state(), before);
        let origin = Origin::agent("bus-user");
        e.execute_typed(input, origin.clone()).unwrap();
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        let graph = e.all_schematic_connectivity().unwrap();
        let joined = graph.nets.iter().find(|n| n.members.iter().any(|m| m.id == wire)).unwrap();
        assert!(joined.members.iter().any(|m| m.id == label));
        assert!(!joined.members.iter().any(|m| m.id == untouched));
        assert_eq!(e.read(|p| p.board.clone()), before.project.board);
        e.undo_own(origin).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        assert_eq!(e.with_engine(|e| e.log().replay()), before.project);
    }
    #[test]
    fn incidental_wire_contact_refuses_even_when_target_union_is_explicit() {
        let _guard = LOCK.lock().unwrap();
        let (e, sheet, bus) = fixture();
        let mut pj = e.read(Clone::clone);
        add_wire(&mut pj, sheet, p(4, 8), p(8, 8));
        add_label(&mut pj, sheet, p(100, 100), "DATA0", LabelScope::Local);
        add_wire(&mut pj, sheet, p(3, 6), p(4, 6));
        let e = EngineHandle::new(pj);
        let before = e.session_state();
        let input = unfold(&e, sheet, bus);
        assert!(e.prepare_schematic_bus_unfold(input.clone()).unwrap().run_preview().unwrap_err().to_string().contains("unrelated"));
        assert!(e.execute_typed(input, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
    }
    #[test]
    fn global_only_equal_name_is_not_an_authorized_union() {
        let _guard = LOCK.lock().unwrap();
        let (e, sheet, bus) = fixture();
        let mut pj = e.read(Clone::clone);
        let global = add_label(&mut pj, sheet, p(100, 100), "DATA0", LabelScope::Global);
        let e = EngineHandle::new(pj);
        let input = unfold(&e, sheet, bus);
        let before = e.session_state();
        assert!(e.prepare_schematic_bus_unfold(input.clone()).unwrap().run_preview().is_err());
        assert!(e.execute_typed(input, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
        let graph = e.all_schematic_connectivity().unwrap();
        assert_eq!(graph.nets[0].members[0].id, global);
        assert_eq!(graph.total_nets, 1);
    }
    #[test]
    fn delete_edit_ownership_and_stale_full_cursor_are_atomic() {
        let _guard = LOCK.lock().unwrap();
        let (e, sheet, bus) = fixture();
        let stale = unfold(&e, sheet, bus);
        let s = source(&e, sheet);
        e.execute_typed(
            SetSchematicBus { source: s, bus_id: Some(bus), expression: "DATA[1..0]".into(), start: p(0, 0), end: p(11, 0) },
            Origin::User,
        )
        .unwrap();
        let before = e.session_state();
        assert!(e.execute_typed(stale, Origin::agent("peer")).is_err());
        assert_eq!(e.session_state(), before);
        e.execute_typed(unfold(&e, sheet, bus), Origin::User).unwrap();
        let before = e.session_state();
        assert!(e.execute_typed(DeleteSchematicBus { source: source(&e, sheet), bus_id: bus }, Origin::User).is_err());
        assert!(
            e.execute_typed(
                SetSchematicBus { source: source(&e, sheet), bus_id: Some(bus), expression: "OTHER[0..1]".into(), start: p(0, 0), end: p(10, 0) },
                Origin::User
            )
            .is_err()
        );
        assert_eq!(e.session_state(), before);
        let entry = e.read(|p| *p.schematic.sheets[&sheet].content.as_ref().unwrap().bus_entries.keys().next().unwrap());
        e.execute_typed(DeleteSchematicBusBreakout { source: source(&e, sheet), entry_id: entry }, Origin::User).unwrap();
        e.execute_typed(DeleteSchematicBus { source: source(&e, sheet), bus_id: bus }, Origin::User).unwrap();
        assert!(e.read(|p| { p.schematic.sheets[&sheet].content.as_ref().unwrap().buses.is_empty() }));
    }
    #[test]
    fn two_slot_retained_work_and_finite_collision_exhaustion() {
        let _guard = LOCK.lock().unwrap();
        let (e, _, _) = fixture();
        let a = e.prepare_schematic_buses(&SchematicBusesQuery {}).unwrap();
        let b = e.prepare_schematic_buses(&SchematicBusesQuery {}).unwrap();
        assert!(e.prepare_schematic_buses(&SchematicBusesQuery {}).is_err());
        drop(a);
        let c = e.prepare_schematic_buses(&SchematicBusesQuery {}).unwrap();
        drop(b);
        drop(c);
        assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
        let id = ObjectId::new();
        let mut ids = BTreeSet::from([id]);
        let mut calls = 0;
        assert!(
            fresh(&mut ids, || {
                calls += 1;
                id
            })
            .unwrap_err()
            .to_string()
            .contains("1024")
        );
        assert_eq!(calls, 1024);
        assert_eq!(ids.len(), 1);
    }
}
