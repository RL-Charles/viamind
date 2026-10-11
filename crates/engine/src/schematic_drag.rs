//! Guarded single-symbol translations with exact preserved native graph anchors.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Command, CommandError, Delta, EngineHandle, EventCursor, SchematicConnectivityQuery, SchematicConnectivityResult, SchematicMemberKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, SchematicContent, SchematicWire},
};
use viamind_geom::Nm;

const MAX_VISITS: usize = 1_000_000;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_drag", message)
}
struct Permit;
impl Permit {
    fn reserve() -> Result<Self, CommandError> {
        let mut active = ACTIVE.load(Ordering::Acquire);
        loop {
            if active >= 2 {
                return Err(invalid("two connected-drag planners are busy; retry after completion"));
            }
            match ACTIVE.compare_exchange_weak(active, active + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(Self),
                Err(current) => active = current,
            }
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DragSchematicSymbol {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub sheet_id: ObjectId,
    pub symbol_id: ObjectId,
    pub delta: Coordinate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchematicDragSegment {
    pub original_wire_id: ObjectId,
    /// None retains original_wire_id; Some identifies a private new bend segment.
    pub new_segment_ordinal: Option<usize>,
    pub start: Coordinate,
    pub end: Coordinate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchematicDragPreview {
    pub expected_project: ObjectId,
    pub source_cursor: EventCursor,
    pub sheet_id: ObjectId,
    pub symbol_id: ObjectId,
    pub original_position: Coordinate,
    pub target_position: Coordinate,
    pub segments: Vec<SchematicDragSegment>,
    pub added_wire_count: usize,
    pub original_graph_members: usize,
    pub exact_candidate_visits: usize,
    pub native_graph_partition_preserved: bool,
    pub scope: String,
}
/// The actual blocking worker retains admission after an async caller disappears.
pub struct SchematicDragWork {
    engine: EngineHandle,
    request: DragSchematicSymbol,
    _permit: Permit,
}
impl EngineHandle {
    pub fn prepare_schematic_symbol_drag(&self, request: DragSchematicSymbol) -> Result<SchematicDragWork, CommandError> {
        crate::command_hooks::check_read().map_err(|e| invalid(e.to_string()))?;
        self.with_engine(|engine| request.check_source(engine.project(), engine.event_cursor()))?;
        Ok(SchematicDragWork { engine: self.clone(), request, _permit: Permit::reserve()? })
    }
}
impl SchematicDragWork {
    pub fn run(self) -> Result<SchematicDragPreview, CommandError> {
        let bytes = self.execute()?;
        serde_json::from_slice(&bytes).map_err(|e| invalid(format!("preview result: {e}")))
    }
    pub fn run_value(self) -> Result<serde_json::Value, CommandError> {
        let bytes = self.execute()?;
        serde_json::from_slice(&bytes).map_err(|e| invalid(format!("preview result: {e}")))
    }
    fn execute(&self) -> Result<Vec<u8>, CommandError> {
        crate::command_hooks::check_read().map_err(|e| invalid(e.to_string()))?;
        let capture = self.engine.with_engine(|engine| {
            self.request.check_source(engine.project(), engine.event_cursor())?;
            capture(engine.project(), self.request)
        })?;
        let plan = plan(capture, self.request, MAX_VISITS)?;
        serialize(&plan.preview, MAX_OUTPUT)
    }
}
struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("drag preview exceeds output budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn serialize(value: &SchematicDragPreview, limit: usize) -> Result<Vec<u8>, CommandError> {
    let mut writer = BoundedWriter { bytes: vec![], limit };
    serde_json::to_writer(&mut writer, value).map_err(|e| invalid(format!("bounded preview: {e}")))?;
    Ok(writer.bytes)
}
impl DragSchematicSymbol {
    fn check_source(&self, project: &Project, cursor: EventCursor) -> Result<(), CommandError> {
        if project.id != self.expected_project || cursor != self.source_cursor {
            return Err(CommandError::conflict("source changed; restart connected drag"));
        }
        if [self.expected_project, self.sheet_id, self.symbol_id].iter().any(|id| id.as_uuid().is_nil()) {
            return Err(invalid("identities must be nonnil"));
        }
        self.delta.validate().map_err(invalid)
    }
}
impl Command for DragSchematicSymbol {
    const NAME: &'static str = "drag_schematic_symbol";
    const DESCRIPTION: &'static str = "Translate one explicit schematic symbol and its cardinal endpoint-attached wires, preserving the complete bounded native graph's original wire/pin/junction/label partition. Required exact project/full source cursor, sheet/symbol UUID and integer Nm delta. One sheet transaction/undo; package membership, PCB and metadata unchanged. At most256 pins/incident wires; unsupported interior/direct-point/diagonal attachments or changed topology refuse atomically. Complete schematic: 10000 graph objects/2 MiB text, 100000 combined identities, shared1million visits and2 MiB preview bounds. Preview with preview_schematic_symbol_drag; apply recalculates, never accepts a supplied pass flag. No ERC/DRC certification.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.source_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        if project.id != self.expected_project {
            return Err(CommandError::conflict("project changed; restart connected drag"));
        }
        let _permit = Permit::reserve()?;
        let mut plan = plan(capture(project, self)?, self, MAX_VISITS)?;
        serialize(&plan.preview, MAX_OUTPUT)?;
        if self.delta.x.0 == 0 && self.delta.y.0 == 0 {
            return Ok(vec![]);
        }
        let content = plan.sheet.content.as_mut().expect("selected symbol requires content");
        for temporary in &plan.new_ids {
            let mut wire = content.wires.remove(temporary).expect("planned new segment");
            let id = (0..16)
                .map(|_| ObjectId::new())
                .find(|id| !plan.ids.contains(id))
                .ok_or_else(|| invalid("fresh wire identity collision budget exhausted"))?;
            plan.ids.insert(id);
            wire.id = id;
            content.wires.insert(id, wire);
        }
        // The bounded preflight preceded even the first native ID-set allocation.
        crate::schematic_budget::project(project, plan.new_ids.len(), "connected drag commit")?;
        viamind_doc::schematic::validate_project(project, Some(&plan.sheet)).map_err(invalid)?;
        Ok(vec![Delta::record_sheet(project, self.sheet_id, Some(plan.sheet))])
    }
}
struct Capture {
    snapshot: crate::schematic_connectivity::Snapshot,
    ids: BTreeSet<ObjectId>,
    global_rows: usize,
}
fn capture(project: &Project, input: DragSchematicSymbol) -> Result<Capture, CommandError> {
    input.delta.validate().map_err(invalid)?;
    let sheet = project.schematic.sheets.get(&input.sheet_id).ok_or_else(|| CommandError::not_found("sheet", input.sheet_id))?;
    let symbol =
        sheet.content.as_ref().and_then(|c| c.symbols.get(&input.symbol_id)).ok_or_else(|| CommandError::not_found("symbol", input.symbol_id))?;
    if symbol.pins.len() > 256 {
        return Err(invalid("connected drag supports at most 256 selected symbol pins"));
    }
    crate::schematic_budget::project(project, 0, "connected drag identities")?;
    crate::schematic_budget::sheet(sheet, "connected drag sheet")?;
    // capture checks the stronger whole-graph borrowed limits before validation/copies.
    let snapshot = crate::schematic_connectivity::capture(project, input.source_cursor)?;
    viamind_doc::schematic::validate_project(project, None).map_err(invalid)?;
    let mut ids = BTreeSet::from([project.id]);
    for p in project.board.parts.values() {
        ids.insert(p.id);
        for pad in p.pads.iter().flat_map(|pads| &pads.0) {
            ids.insert(pad.id);
        }
    }
    ids.extend(project.board.tracks.keys().chain(project.board.vias.keys()).copied());
    for sheet in &snapshot.sheets {
        ids.insert(sheet.id);
        if let Some(c) = &sheet.content {
            ids.extend(c.packages.keys().chain(c.symbols.keys()).chain(c.wires.keys()).chain(c.junctions.keys()).chain(c.labels.keys()).copied());
            for symbol in c.symbols.values() {
                ids.extend(symbol.pins.iter().map(|p| p.id));
            }
        }
    }
    let mut global_rows = 1 + project.board.parts.len() + project.board.tracks.len() + project.board.vias.len() + project.schematic.sheets.len();
    for part in project.board.parts.values() {
        global_rows += part.pads.as_ref().map_or(0, |p| p.0.len());
    }
    for sheet in &snapshot.sheets {
        if let Some(c) = &sheet.content {
            let (records, _) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            global_rows += c.object_count() + c.pin_count() + records.saturating_sub(c.packages.len());
        }
    }
    Ok(Capture { snapshot, ids, global_rows })
}
fn charge(remaining: &mut usize) -> Result<(), CommandError> {
    *remaining = remaining.checked_sub(1).ok_or_else(|| invalid("connected drag exceeded cumulative 1000000 exact candidate visits"))?;
    Ok(())
}
fn translated(p: Coordinate, d: Coordinate) -> Result<Coordinate, CommandError> {
    let p = Coordinate {
        x: Nm(p.x.0.checked_add(d.x.0).ok_or_else(|| invalid("X addition overflow"))?),
        y: Nm(p.y.0.checked_add(d.y.0).ok_or_else(|| invalid("Y addition overflow"))?),
    };
    p.validate().map_err(invalid)?;
    Ok(p)
}
fn on_segment(p: Coordinate, a: Coordinate, b: Coordinate) -> bool {
    let (x, y, ax, ay, bx, by) = (i128::from(p.x.0), i128::from(p.y.0), i128::from(a.x.0), i128::from(a.y.0), i128::from(b.x.0), i128::from(b.y.0));
    (x - ax) * (by - ay) == (y - ay) * (bx - ax) && x >= ax.min(bx) && x <= ax.max(bx) && y >= ay.min(by) && y <= ay.max(by)
}
fn attachment_points(c: &SchematicContent, symbol_id: ObjectId, remaining: &mut usize) -> Result<BTreeSet<CoordinateKey>, CommandError> {
    let symbol = &c.symbols[&symbol_id];
    let mut points = BTreeSet::new();
    for pin in &symbol.pins {
        let p = symbol.pin_position(pin.position).map_err(invalid)?;
        if !points.insert(key(p)) {
            return Err(invalid("selected symbol has coincident pin positions"));
        }
        for wire in c.wires.values() {
            charge(remaining)?;
            if p != wire.start && p != wire.end && on_segment(p, wire.start, wire.end) {
                return Err(invalid("selected pin lies inside a wire; endpoint attachments only"));
            }
        }
        for position in c.junctions.values().map(|j| j.position).chain(c.labels.values().map(|l| l.position)) {
            charge(remaining)?;
            if p == position {
                return Err(invalid("selected pin directly touches a label or junction; endpoint wire attachments only"));
            }
        }
        for other in c.symbols.values().filter(|s| s.id != symbol_id) {
            for other_pin in &other.pins {
                charge(remaining)?;
                if p == other.pin_position(other_pin.position).map_err(invalid)? {
                    return Err(invalid("selected pin directly touches another symbol pin"));
                }
            }
        }
    }
    Ok(points)
}
type CoordinateKey = (i64, i64);
fn key(p: Coordinate) -> CoordinateKey {
    (p.x.0, p.y.0)
}
struct Plan {
    sheet: Sheet,
    preview: SchematicDragPreview,
    new_ids: Vec<ObjectId>,
    ids: BTreeSet<ObjectId>,
}
fn plan(capture: Capture, input: DragSchematicSymbol, limit: usize) -> Result<Plan, CommandError> {
    let Capture { snapshot, mut ids, global_rows } = capture;
    let mut remaining = limit;
    let source = snapshot.sheets.iter().find(|s| s.id == input.sheet_id).expect("captured selected sheet");
    let c = source.content.as_ref().expect("captured selected symbol");
    let original_position = c.symbols[&input.symbol_id].position;
    let target_position = translated(original_position, input.delta)?;
    let points = attachment_points(c, input.symbol_id, &mut remaining)?;
    // Determine additions from borrowed original geometry before candidate copies/IDs.
    let mut additions = 0usize;
    let mut attachments = 0usize;
    for wire in c.wires.values() {
        charge(&mut remaining)?;
        let a = points.contains(&key(wire.start));
        let b = points.contains(&key(wire.end));
        if !a && !b {
            continue;
        }
        attachments += 1;
        if attachments > 256 {
            return Err(invalid("connected drag supports at most 256 incident wires"));
        }
        if wire.start == wire.end || (wire.start.x != wire.end.x && wire.start.y != wire.end.y) {
            return Err(invalid("incident wires must be nonzero horizontal or vertical segments"));
        }
        let start = if a { translated(wire.start, input.delta)? } else { wire.start };
        let end = if b { translated(wire.end, input.delta)? } else { wire.end };
        if start == end {
            return Err(invalid("drag collapses an original wire"));
        }
        additions += usize::from(a != b && start.x != end.x && start.y != end.y);
    }
    let mut graph_rows = snapshot.sheets.len();
    for sheet in &snapshot.sheets {
        if let Some(c) = &sheet.content {
            let (records, _) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            graph_rows += c.object_count() + c.pin_count() + records.saturating_sub(c.packages.len());
        }
    }
    if graph_rows.saturating_add(additions) > 10_000
        || global_rows.saturating_add(additions) > 100_000
        || c.object_count().saturating_add(additions) > 10_000
    {
        return Err(invalid("planned additions exceed graph/sheet/global identity budget"));
    }
    let mut candidate = snapshot.clone();
    let sheet = candidate.sheets.iter_mut().find(|s| s.id == input.sheet_id).expect("candidate sheet");
    let content = sheet.content.as_mut().expect("candidate content");
    let symbol = content.symbols.get_mut(&input.symbol_id).expect("candidate symbol");
    symbol.position = target_position;
    symbol.validate().map_err(invalid)?;
    let mut segments = Vec::new();
    let mut new_ids = Vec::new();
    let mut new_origins = BTreeMap::new();
    let mut temporary = 1u128;
    let mut incident = 0usize;
    for wire in c.wires.values() {
        charge(&mut remaining)?;
        let a = points.contains(&key(wire.start));
        let b = points.contains(&key(wire.end));
        if !a && !b {
            continue;
        }
        incident += 1;
        if incident > 256 {
            return Err(invalid("connected drag supports at most 256 incident wires"));
        }
        if wire.start == wire.end || (wire.start.x != wire.end.x && wire.start.y != wire.end.y) {
            return Err(invalid("incident wires must be nonzero horizontal or vertical segments"));
        }
        let start = if a { translated(wire.start, input.delta)? } else { wire.start };
        let end = if b { translated(wire.end, input.delta)? } else { wire.end };
        if start == end {
            return Err(invalid("drag collapses an original wire"));
        }
        if a && b || start.x == end.x || start.y == end.y {
            content.wires.insert(wire.id, SchematicWire { id: wire.id, start, end });
            segments.push(SchematicDragSegment { original_wire_id: wire.id, new_segment_ordinal: None, start, end });
        } else {
            let (fixed, moving) = if a { (end, start) } else { (start, end) };
            let bend = if wire.start.y == wire.end.y { Coordinate { x: moving.x, y: fixed.y } } else { Coordinate { x: fixed.x, y: moving.y } };
            bend.validate().map_err(invalid)?;
            let (old_start, old_end, new_start, new_end) = if a { (bend, fixed, moving, bend) } else { (fixed, bend, bend, moving) };
            content.wires.insert(wire.id, SchematicWire { id: wire.id, start: old_start, end: old_end });
            let id = loop {
                let id = format!("{temporary:032x}").parse::<ObjectId>().map_err(|e| invalid(e.to_string()))?;
                temporary += 1;
                if ids.insert(id) {
                    break id;
                }
            };
            let ordinal = new_ids.len();
            new_ids.push(id);
            new_origins.insert(id, wire.id);
            content.wires.insert(id, SchematicWire { id, start: new_start, end: new_end });
            segments.push(SchematicDragSegment { original_wire_id: wire.id, new_segment_ordinal: None, start: old_start, end: old_end });
            segments.push(SchematicDragSegment { original_wire_id: wire.id, new_segment_ordinal: Some(ordinal), start: new_start, end: new_end });
        }
    }
    if content.object_count() > 10_000 {
        return Err(invalid("candidate would exceed sheet object budget"));
    }
    // Check additions against graph totals before any candidate graph secondary allocations.
    let mut graph_count = candidate.sheets.len();
    for s in &candidate.sheets {
        if let Some(c) = &s.content {
            let (records, _) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            graph_count =
                graph_count.saturating_add(c.object_count()).saturating_add(c.pin_count()).saturating_add(records.saturating_sub(c.packages.len()));
        }
    }
    if graph_count > 10_000 {
        return Err(invalid("candidate would exceed10000 complete graph objects"));
    }
    let sheet = candidate.sheets.iter().find(|s| s.id == input.sheet_id).expect("candidate sheet").clone();
    crate::schematic_budget::sheet(&sheet, "connected drag candidate")?;
    let query = SchematicConnectivityQuery { offset: 0, limit: 10_000 };
    let before = crate::schematic_connectivity::calculate_with_budget(snapshot, &query, &mut remaining)?;
    let after = crate::schematic_connectivity::calculate_with_budget(candidate, &query, &mut remaining)?;
    preserve(&before, &after, &new_origins)?;
    let preview = SchematicDragPreview {
        expected_project: input.expected_project,
        source_cursor: input.source_cursor,
        sheet_id: input.sheet_id,
        symbol_id: input.symbol_id,
        original_position,
        target_position,
        segments,
        added_wire_count: new_ids.len(),
        original_graph_members: before.coverage.members,
        exact_candidate_visits: limit - remaining,
        native_graph_partition_preserved: true,
        scope: "Exact original native flat graph member partition and label scopes preserved; no ERC/DRC or physical continuity certification".into(),
    };
    Ok(Plan { sheet, preview, new_ids, ids })
}
type Anchor = (ObjectId, ObjectId, u8);
fn anchor(member: &crate::SchematicNetMember) -> Anchor {
    let kind = match member.kind {
        SchematicMemberKind::Wire => 0,
        SchematicMemberKind::Pin => 1,
        SchematicMemberKind::Junction => 2,
        SchematicMemberKind::Label => 3,
    };
    (member.sheet_id, member.id, kind)
}
type Names = Vec<(ObjectId, ObjectId, String, u8)>;
fn partition(graph: &SchematicConnectivityResult, original: &BTreeSet<Anchor>) -> Result<BTreeMap<Vec<Anchor>, Names>, CommandError> {
    if graph.next_offset.is_some() {
        return Err(invalid("complete graph proof unexpectedly paginated"));
    }
    let mut result = BTreeMap::new();
    let mut found = BTreeSet::new();
    for net in &graph.nets {
        let mut members = Vec::new();
        for member in &net.members {
            let key = anchor(member);
            if original.contains(&key) {
                members.push(key);
                found.insert(key);
            }
        }
        if members.is_empty() {
            return Err(invalid("candidate creates an unanchored component"));
        }
        members.sort_unstable();
        let mut names = net
            .names
            .iter()
            .map(|n| {
                (
                    n.sheet_id,
                    n.label_id,
                    n.name.clone(),
                    match n.scope {
                        viamind_doc::schematic::LabelScope::Local => 0,
                        viamind_doc::schematic::LabelScope::Global => 1,
                        viamind_doc::schematic::LabelScope::Power => 2,
                    },
                )
            })
            .collect::<Vec<_>>();
        names.sort();
        if result.insert(members, names).is_some() {
            return Err(invalid("duplicate canonical graph component"));
        }
    }
    if &found != original {
        return Err(invalid("candidate loses original graph anchors"));
    }
    Ok(result)
}
fn preserve(
    before: &SchematicConnectivityResult,
    after: &SchematicConnectivityResult,
    new_origins: &BTreeMap<ObjectId, ObjectId>,
) -> Result<(), CommandError> {
    let original = before.nets.iter().flat_map(|n| n.members.iter().map(anchor)).collect::<BTreeSet<_>>();
    if partition(before, &original)? != partition(after, &original)? {
        return Err(invalid("candidate changes original graph connectivity; choose another displacement"));
    }
    for (new, old) in new_origins {
        if !after.nets.iter().any(|n| n.members.iter().any(|m| m.id == *new) && n.members.iter().any(|m| m.id == *old)) {
            return Err(invalid("new segment left its original wire component"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, Origin};
    use std::sync::Mutex;
    use viamind_doc::schematic::{LabelScope, PinType, SchematicLabel, SchematicSymbol, SymbolPin};
    static LOCK: Mutex<()> = Mutex::new(());
    fn p(x: i64, y: i64) -> Coordinate {
        Coordinate { x: Nm(x), y: Nm(y) }
    }
    fn add_symbol(c: &mut SchematicContent, position: Coordinate) -> ObjectId {
        let id = ObjectId::new();
        c.symbols.insert(
            id,
            SchematicSymbol {
                id,
                designator: format!("U{}", c.symbols.len() + 1),
                value: String::new(),
                footprint: None,
                position,
                rotation_quarters: 0,
                mirrored: false,
                width: Nm(2),
                height: Nm(2),
                pins: vec![SymbolPin {
                    id: ObjectId::new(),
                    number: "1".into(),
                    name: String::new(),
                    position: p(0, 0),
                    electrical: PinType::Passive,
                }],
            },
        );
        id
    }
    fn wire(c: &mut SchematicContent, start: Coordinate, end: Coordinate) -> ObjectId {
        let id = ObjectId::new();
        c.wires.insert(id, SchematicWire { id, start, end });
        id
    }
    fn fixture() -> (Project, ObjectId, ObjectId, ObjectId) {
        let mut project = Project::new("Drag");
        let mut sheet = Sheet::new("Main");
        let mut c = SchematicContent::default();
        let moved = add_symbol(&mut c, p(0, 0));
        add_symbol(&mut c, p(20, 0));
        let wire = wire(&mut c, p(0, 0), p(20, 0));
        sheet.content = Some(c);
        let id = sheet.id;
        project.schematic.sheets.insert(id, sheet);
        (project, id, moved, wire)
    }
    fn request(engine: &EngineHandle, sheet: ObjectId, symbol: ObjectId, delta: Coordinate) -> DragSchematicSymbol {
        let (expected_project, source_cursor) = engine.with_engine(|e| (e.project().id, e.event_cursor()));
        DragSchematicSymbol { expected_project, source_cursor, sheet_id: sheet, symbol_id: symbol, delta }
    }
    #[test]
    fn cardinal_bend_preview_is_immutable_and_commit_undo_redo_replay_are_exact() {
        let _lock = LOCK.lock().unwrap();
        let (project, sheet, symbol, wire) = fixture();
        let engine = EngineHandle::new(project.clone());
        let original = engine.session_state();
        let input = request(&engine, sheet, symbol, p(3, 7));
        let preview = engine.prepare_schematic_symbol_drag(input).unwrap().run().unwrap();
        assert_eq!(preview.target_position, p(3, 7));
        assert_eq!(preview.added_wire_count, 1);
        assert_eq!(preview.segments[0], SchematicDragSegment { original_wire_id: wire, new_segment_ordinal: None, start: p(3, 0), end: p(20, 0) });
        assert_eq!(preview.segments[1].start, p(3, 7));
        assert_eq!(preview.segments[1].end, p(3, 0));
        assert_eq!(engine.session_state(), original);
        let origin = Origin::agent("drag");
        let result = engine.execute_typed(input, origin.clone()).unwrap();
        assert_eq!(result.deltas.len(), 1);
        let after = engine.session_state();
        assert_eq!(after.cursor.sequence, original.cursor.sequence + 1);
        assert_eq!(after.project.schematic.sheets[&sheet].content.as_ref().unwrap().wires[&wire].start, p(3, 0));
        assert_eq!(engine.with_engine(|e| e.log().replay()), after.project);
        engine.undo_own(origin.clone()).unwrap();
        assert_eq!(engine.session_state().project, project);
        engine.redo_own(origin).unwrap();
        assert_eq!(engine.session_state().project, after.project);
    }
    #[test]
    fn aligned_stretch_and_both_attached_endpoints_preserve_wire_ids() {
        let _lock = LOCK.lock().unwrap();
        let (project, sheet, symbol, _wire) = fixture();
        let engine = EngineHandle::new(project);
        let preview = engine.prepare_schematic_symbol_drag(request(&engine, sheet, symbol, p(5, 0))).unwrap().run().unwrap();
        assert_eq!(preview.added_wire_count, 0);
        assert_eq!(preview.segments[0].start, p(5, 0));
        let (mut project, sheet, symbol, wire) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        c.symbols.retain(|id, _| *id == symbol);
        let moved = c.symbols.get_mut(&symbol).unwrap();
        moved.pins.push(SymbolPin { id: ObjectId::new(), number: "2".into(), name: String::new(), position: p(20, 0), electrical: PinType::Passive });
        let engine = EngineHandle::new(project);
        let preview = engine.prepare_schematic_symbol_drag(request(&engine, sheet, symbol, p(4, 9))).unwrap().run().unwrap();
        assert_eq!(preview.added_wire_count, 0);
        assert_eq!(preview.segments[0].original_wire_id, wire);
        assert_eq!(preview.segments[0].start, p(4, 9));
        assert_eq!(preview.segments[0].end, p(24, 9));
    }
    #[test]
    fn asymmetric_mirrored_quarter_turn_uses_native_pin_positions_and_vertical_bend() {
        let _lock = LOCK.lock().unwrap();
        let (mut project, sheet, symbol, old_wire) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        let moved = c.symbols.get_mut(&symbol).unwrap();
        moved.position = p(10, -20);
        moved.rotation_quarters = 1;
        moved.mirrored = true;
        moved.pins[0].position = p(2, 1);
        assert_eq!(moved.pin_position(p(2, 1)).unwrap(), p(11, -18));
        c.symbols.retain(|id, _| *id == symbol);
        c.wires.get_mut(&old_wire).unwrap().start = p(11, -18);
        c.wires.get_mut(&old_wire).unwrap().end = p(11, 0);
        let engine = EngineHandle::new(project);
        let preview = engine.prepare_schematic_symbol_drag(request(&engine, sheet, symbol, p(3, 5))).unwrap().run().unwrap();
        assert_eq!(preview.segments[0].start, p(11, -13));
        assert_eq!(preview.segments[0].end, p(11, 0));
        assert_eq!(preview.segments[1].start, p(14, -13));
        assert_eq!(preview.segments[1].end, p(11, -13));
    }
    #[test]
    fn unsupported_attachments_and_accidental_net_merge_refuse_exact_state() {
        let _lock = LOCK.lock().unwrap();
        for kind in 0..5 {
            let (mut project, sheet, symbol, old_wire) = fixture();
            let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
            match kind {
                0 => {
                    c.wires.get_mut(&old_wire).unwrap().start = p(-10, 0);
                }
                1 => {
                    let id = ObjectId::new();
                    c.labels.insert(id, SchematicLabel { id, position: p(0, 0), name: "N".into(), scope: LabelScope::Local });
                }
                2 => {
                    c.wires.get_mut(&old_wire).unwrap().end = p(20, 10);
                }
                3 => {
                    wire(c, p(3, 4), p(8, 4));
                }
                _ => {
                    add_symbol(c, p(0, 0));
                }
            }
            let engine = EngineHandle::new(project);
            let before = engine.session_state();
            let result = engine.prepare_schematic_symbol_drag(request(&engine, sheet, symbol, p(3, 7))).unwrap().run();
            assert!(result.is_err(), "case{kind}");
            assert_eq!(engine.session_state(), before);
        }
    }
    #[test]
    fn same_net_count_different_actual_member_partition_is_rejected() {
        let _lock = LOCK.lock().unwrap();
        let (mut project, sheet, moved, _) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        c.symbols.retain(|id, _| *id == moved);
        let other = add_symbol(c, p(0, 20));
        wire(c, p(0, 20), p(20, 20));
        let engine = Engine::new(project.clone());
        let cursor = engine.event_cursor();
        let before = crate::schematic_connectivity::project_schematic_connectivity(&project, cursor).unwrap();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        c.symbols.get_mut(&moved).unwrap().position = p(0, 20);
        c.symbols.get_mut(&other).unwrap().position = p(0, 0);
        let after = crate::schematic_connectivity::project_schematic_connectivity(&project, cursor).unwrap();
        assert_eq!(before.total_nets, after.total_nets);
        assert_eq!(before.total_nets, 2);
        assert!(preserve(&before, &after, &BTreeMap::new()).is_err());
    }
    #[test]
    fn stale_and_exhausted_work_output_and_admission_refuse_without_changes() {
        let _lock = LOCK.lock().unwrap();
        let (project, sheet, symbol, _) = fixture();
        let engine = EngineHandle::new(project);
        let input = request(&engine, sheet, symbol, p(3, 7));
        let before = engine.session_state();
        assert!(plan(engine.with_engine(|e| capture(e.project(), input)).unwrap(), input, 0).is_err());
        assert_eq!(engine.session_state(), before);
        let first = engine.prepare_schematic_symbol_drag(input).unwrap();
        let second = engine.prepare_schematic_symbol_drag(input).unwrap();
        assert!(engine.prepare_schematic_symbol_drag(input).is_err());
        drop(second);
        let result = first.run().unwrap();
        assert!(serialize(&result, 1).is_err());
        assert_eq!(engine.session_state(), before);
        engine.execute("add_part", serde_json::json!({"designator":"R9","value":"","footprint":"F","x_mm":0,"y_mm":0}), Origin::User).unwrap();
        let state = engine.session_state();
        assert!(engine.prepare_schematic_symbol_drag(input).is_err());
        assert!(engine.execute_typed(input, Origin::User).is_err());
        assert_eq!(engine.session_state(), state);
        assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
    }
    #[test]
    fn planned_additions_preflight_graph_budget_and_zero_delta_is_noop() {
        let _lock = LOCK.lock().unwrap();
        let (project, sheet, symbol, _) = fixture();
        let engine = EngineHandle::new(project);
        let before = engine.session_state();
        engine.execute_typed(request(&engine, sheet, symbol, p(0, 0)), Origin::User).unwrap();
        assert_eq!(engine.session_state(), before);
        let (mut project, sheet, symbol, _) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        for i in 0..9994 {
            let id = ObjectId::new();
            c.junctions.insert(id, viamind_doc::schematic::SchematicJunction { id, position: p(100 + i, 100) });
        }
        let engine = EngineHandle::new(project);
        let input = request(&engine, sheet, symbol, p(3, 7));
        let before = engine.session_state();
        let error = engine.prepare_schematic_symbol_drag(input).unwrap().run().unwrap_err();
        assert!(error.to_string().contains("planned additions"));
        assert_eq!(engine.session_state(), before);
    }
    #[test]
    fn pre_hooks_receive_final_candidate_veto_atomically_and_captured_work_refuses_reentry() {
        let _lock = LOCK.lock().unwrap();
        let (project, sheet, symbol, _) = fixture();
        let e = EngineHandle::new(project);
        let input = request(&e, sheet, symbol, p(3, 7));
        let held = std::sync::Arc::new(std::sync::Mutex::new(Some(e.prepare_schematic_symbol_drag(input).unwrap())));
        let callback = e.clone();
        let work = held.clone();
        e.register_pre_command_hook(
            "drag-veto".into(),
            std::sync::Arc::new(move |candidate| {
                assert_eq!(candidate.project.schematic.sheets[&sheet].content.as_ref().unwrap().symbols[&symbol].position, p(3, 7));
                assert!(callback.prepare_schematic_symbol_drag(input).is_err());
                assert!(work.lock().unwrap().take().unwrap().run().unwrap_err().to_string().contains("pre-command"));
                Err("explicit drag veto".into())
            }),
        )
        .unwrap();
        let before = e.session_state();
        assert!(e.execute_typed(input, Origin::User).unwrap_err().to_string().contains("explicit drag veto"));
        assert_eq!(e.session_state(), before);
        assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
        e.remove_command_hook("drag-veto").unwrap();
        e.execute_typed(input, Origin::User).unwrap();
    }
    #[test]
    fn unrelated_huge_invalid_board_metadata_is_not_copied_or_validated() {
        let _lock = LOCK.lock().unwrap();
        let (mut project, sheet, symbol, _) = fixture();
        let part = viamind_doc::Part {
            artwork: None,
            assembly: Some(Box::new(viamind_doc::AssemblyMetadata { manufacturer: Some("x".repeat(3 * 1024 * 1024)), ..Default::default() })),
            schematic_symbol: None,
            schematic_package: None,
            pads: None,
            id: ObjectId::new(),
            designator: "R1".into(),
            value: String::new(),
            footprint: "F".into(),
            position: viamind_geom::Point::new(Nm(0), Nm(0)),
            rotation_mdeg: 0,
            side: viamind_doc::Side::Top,
        };
        let id = part.id;
        project.board.parts.insert(id, part);
        let e = EngineHandle::new(project);
        let before = e.event_cursor();
        let result = e.prepare_schematic_symbol_drag(request(&e, sheet, symbol, p(3, 7))).unwrap().run().unwrap();
        assert!(result.native_graph_partition_preserved);
        assert_eq!(e.event_cursor(), before);
        assert_eq!(e.read(|p| p.board.parts[&id].assembly.as_ref().unwrap().manufacturer.as_ref().unwrap().len()), 3 * 1024 * 1024);
    }
    #[test]
    fn complete_package_unit_move_keeps_owner_membership_other_units_and_pins_exact() {
        let _lock = LOCK.lock().unwrap();
        let project = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/package-annotation.viamind.json")).unwrap();
        let sheet = "00000000-0000-0000-0000-000000000001".parse().unwrap();
        let symbol = "00000000-0000-0000-0000-000000000068".parse().unwrap();
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let input = request(&e, sheet, symbol, p(1_000_003, 2_000_007));
        assert_eq!(e.prepare_schematic_symbol_drag(input).unwrap().run().unwrap().added_wire_count, 0);
        e.execute_typed(input, Origin::User).unwrap();
        let mut expected = before.project.clone();
        let moved = expected.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap().symbols.get_mut(&symbol).unwrap();
        moved.position = translated(moved.position, input.delta).unwrap();
        assert_eq!(e.session_state().project, expected);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.session_state().project, before.project);
    }
    #[test]
    fn global_labels_across_unselected_sheet_keep_original_partition_and_names() {
        let _lock = LOCK.lock().unwrap();
        let (mut project, sheet, symbol, _) = fixture();
        let c = project.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        let id = ObjectId::new();
        c.labels.insert(id, SchematicLabel { id, position: p(20, 0), name: "GLOBAL".into(), scope: LabelScope::Global });
        let mut other = Sheet::new("Unselected");
        let mut content = SchematicContent::default();
        add_symbol(&mut content, p(100, 100));
        content.symbols.values_mut().next().unwrap().designator = "U3".into();
        wire(&mut content, p(100, 100), p(120, 100));
        let id = ObjectId::new();
        content.labels.insert(id, SchematicLabel { id, position: p(120, 100), name: "GLOBAL".into(), scope: LabelScope::Global });
        other.content = Some(content);
        let other_id = other.id;
        project.schematic.sheets.insert(other.id, other.clone());
        viamind_doc::schematic::validate_project(&project, None).expect("authored cross-sheet fixture is native-valid before planner");
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let input = request(&e, sheet, symbol, p(3, 7));
        let preview = e.prepare_schematic_symbol_drag(input).unwrap().run().unwrap();
        assert_eq!(preview.original_graph_members, 7);
        e.execute_typed(input, Origin::User).unwrap();
        assert_eq!(e.read(|p| p.schematic.sheets[&other_id].clone()), other);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.session_state().project, before.project);
    }
    #[test]
    fn complete_regulator_fixture_keeps_independently_declared_original_net_members_and_names() {
        let _lock = LOCK.lock().unwrap();
        let project = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
        viamind_doc::schematic::validate_project(&project, None).unwrap();
        let id = |n: u16| format!("00000000-0000-0000-0000-{n:012x}").parse::<ObjectId>().unwrap();
        let expected = BTreeMap::from([
            ("VIN".to_owned(), [104, 108, 113, 119].into_iter().map(id).collect::<BTreeSet<_>>()),
            ("3V3".to_owned(), [106, 111, 114, 120].into_iter().map(id).collect::<BTreeSet<_>>()),
            ("GND".to_owned(), [105, 109, 112, 115, 116, 117, 118, 121].into_iter().map(id).collect::<BTreeSet<_>>()),
        ]);
        let original: BTreeSet<_> = expected.values().flatten().copied().collect();
        let check_graph = |graph: &SchematicConnectivityResult| {
            assert!(graph.next_offset.is_none());
            assert_eq!(graph.nets.len(), 3);
            let actual: BTreeMap<_, BTreeSet<_>> = graph
                .nets
                .iter()
                .map(|net| {
                    assert_eq!(net.names.len(), 1);
                    let name = &net.names[0];
                    let (label, scope) = match name.name.as_str() {
                        "VIN" => (119, LabelScope::Local),
                        "3V3" => (120, LabelScope::Local),
                        "GND" => (121, LabelScope::Power),
                        other => panic!("unexpected named net {other}"),
                    };
                    assert_eq!((name.sheet_id, name.label_id, name.scope), (id(102), id(label), scope));
                    assert!(net.members.iter().all(|member| member.sheet_id == id(102)));
                    (name.name.clone(), net.members.iter().map(|member| member.id).filter(|member| original.contains(member)).collect())
                })
                .collect();
            assert_eq!(actual, expected);
        };
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let before_bytes = viamind_doc::file::to_string(&before.project);
        check_graph(&e.query_schematic_connectivity(&crate::SchematicConnectivityQuery { offset: 0, limit: 10_000 }).unwrap());
        let input = request(&e, id(102), id(103), p(1_000_000, -2_000_000));
        let preview = e.prepare_schematic_symbol_drag(input).unwrap().run().unwrap();
        assert_eq!((preview.original_graph_members, preview.added_wire_count), (16, 3));
        assert_eq!(e.session_state(), before);
        e.execute_typed(input, Origin::agent("regulator mover")).unwrap();
        let after = e.session_state();
        assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
        assert_eq!(after.project.board, before.project.board);
        let source = before.project.schematic.sheets[&id(102)].content.as_ref().unwrap();
        let target = after.project.schematic.sheets[&id(102)].content.as_ref().unwrap();
        for fixed in [107, 110] {
            assert_eq!(target.symbols[&id(fixed)], source.symbols[&id(fixed)]);
        }
        for fixed in [115, 116] {
            assert_eq!(target.wires[&id(fixed)], source.wires[&id(fixed)]);
        }
        assert_eq!(target.labels, source.labels);
        assert_eq!(target.junctions, source.junctions);
        assert_eq!(target.packages, source.packages);
        assert_eq!(target.symbols[&id(103)].position, p(21_000_000, 18_000_000));
        let graph = e.query_schematic_connectivity(&crate::SchematicConnectivityQuery { offset: 0, limit: 10_000 }).unwrap();
        assert_eq!(graph.coverage.members, 19);
        check_graph(&graph);
        assert_eq!(e.with_engine(|e| e.log().replay()), after.project);
        e.undo_own(Origin::agent("regulator mover")).unwrap();
        assert_eq!(viamind_doc::file::to_string(&e.session_state().project), before_bytes);
        e.redo_own(Origin::agent("regulator mover")).unwrap();
        assert_eq!(e.session_state().project, after.project);
    }
    #[test]
    fn later_nonconflicting_peer_part_survives_origin_owned_drag_undo_and_redo() {
        let _lock = LOCK.lock().unwrap();
        let (project, sheet, symbol, _) = fixture();
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let mover = Origin::agent("symbol mover");
        let peer = Origin::agent("later PCB peer");
        e.execute_typed(request(&e, sheet, symbol, p(3, 7)), mover.clone()).unwrap();
        e.execute(
            "add_part",
            serde_json::json!({"designator":"R_LATER","value":"preserve peer edit","footprint":"EXPLICIT_PREVIEW","x_mm":1.25,"y_mm":2.5}),
            peer.clone(),
        )
        .unwrap();
        let both = e.session_state();
        assert_eq!(both.project.board.parts.len(), 1);
        let mut expected_undo = before.project.clone();
        expected_undo.board = both.project.board.clone();
        e.undo_own(mover.clone()).unwrap();
        assert_eq!(e.session_state().project, expected_undo, "selective sheet undo must retain the entire later peer board and exact IDs");
        e.redo_own(mover.clone()).unwrap();
        assert_eq!(e.session_state().project, both.project);
        e.undo_own(mover).unwrap();
        e.undo_own(peer).unwrap();
        assert_eq!(e.session_state().project, before.project);
        assert_eq!(e.with_engine(|e| e.log().replay()), before.project);
    }
    #[test]
    fn combined_board_identity_preflight_refuses_before_native_duplicate_validation() {
        let _lock = LOCK.lock().unwrap();
        let (mut project, sheet, symbol, _) = fixture();
        let pad = viamind_doc::Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: viamind_geom::Point::new(Nm(0), Nm(0)),
            width: Nm(1),
            height: Nm(1),
            net: None,
        };
        // Deliberate duplicate poison would fail the native validator, but the
        // borrowed aggregate must reject before that validator allocates ID sets.
        let part = viamind_doc::Part {
            artwork: None,
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            pads: Some(viamind_doc::Pads(vec![pad; 100_001])),
            id: ObjectId::new(),
            designator: "R1".into(),
            value: String::new(),
            footprint: "F".into(),
            position: viamind_geom::Point::new(Nm(0), Nm(0)),
            rotation_mdeg: 0,
            side: viamind_doc::Side::Top,
        };
        let id = part.id;
        project.board.parts.insert(id, part);
        let e = EngineHandle::new(project);
        let cursor = e.event_cursor();
        let result = e.prepare_schematic_symbol_drag(request(&e, sheet, symbol, p(3, 7))).unwrap().run().unwrap_err();
        assert!(result.to_string().contains("100000 combined"), "{result}");
        assert_eq!(e.event_cursor(), cursor);
        assert_eq!(e.read(|p| p.board.parts[&id].pads.as_ref().unwrap().0.len()), 100_001);
    }
}
