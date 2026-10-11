//! Exact, bounded, snapshot-based native schematic graph queries.
use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{
    ObjectId, Sheet,
    schematic::{Coordinate, LabelScope, PinType, validate_sheet},
};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};

use crate::{CommandError, EngineHandle, EventCursor};

const MAX_MEMBERS: usize = 10_000;
const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_CANDIDATES: usize = 1_000_000;
const fn default_limit() -> usize {
    100
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SchematicConnectivityQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
}
impl Default for SchematicConnectivityQuery {
    fn default() -> Self {
        Self { offset: 0, limit: default_limit() }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchematicMemberKind {
    Wire,
    Junction,
    Label,
    Pin,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchematicNetMember {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_key: Option<String>,
    pub sheet_id: ObjectId,
    pub id: ObjectId,
    pub kind: SchematicMemberKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical: Option<PinType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Coordinate>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchematicNetName {
    pub sheet_id: ObjectId,
    pub label_id: ObjectId,
    pub name: String,
    pub scope: LabelScope,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchematicNet {
    /// Minimum (sheet UUID, member UUID); topology edits can change this identity.
    pub id: String,
    pub canonical_name: Option<String>,
    pub aliases: Vec<String>,
    pub multiple_names: bool,
    pub names: Vec<SchematicNetName>,
    pub members: Vec<SchematicNetMember>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchematicConnectivityCoverage {
    pub sheets: usize,
    pub legacy_empty_sheets: usize,
    pub symbols: usize,
    pub symbols_without_pins: usize,
    pub members: usize,
    pub candidate_contacts: usize,
    pub max_objects_including_sheets_and_symbols: usize,
    pub max_text_bytes: usize,
    pub max_candidate_contacts: usize,
    pub exclusions: Vec<&'static str>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchematicConnectivityResult {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub total_nets: usize,
    pub next_offset: Option<usize>,
    pub nets: Vec<SchematicNet>,
    pub coverage: SchematicConnectivityCoverage,
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("schematic_connectivity", message.into())
}
pub(crate) struct Snapshot {
    pub(crate) project_id: ObjectId,
    pub(crate) source_cursor: EventCursor,
    pub(crate) sheets: Vec<Sheet>,
}
impl EngineHandle {
    /// Capture bounded schematic data only; graph calculation releases the engine lock.
    pub fn query_schematic_connectivity(&self, query: &SchematicConnectivityQuery) -> Result<SchematicConnectivityResult, CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        if !(1..=100).contains(&query.limit) {
            return Err(CommandError::invalid("limit", "must be within 1..100"));
        }
        self.schematic_graph(query)
    }
    /// Internal whole-graph result from one capture, bounded by the same 10000-object cap.
    pub(crate) fn all_schematic_connectivity(&self) -> Result<SchematicConnectivityResult, CommandError> {
        self.schematic_graph(&SchematicConnectivityQuery { offset: 0, limit: MAX_MEMBERS })
    }
    fn schematic_graph(&self, query: &SchematicConnectivityQuery) -> Result<SchematicConnectivityResult, CommandError> {
        let snapshot = self.with_engine(|engine| capture(engine.project(), engine.event_cursor()))?;
        calculate(snapshot, query, MAX_CANDIDATES)
    }
}
/// Calculate a whole bounded graph from already captured project data.
pub(crate) fn project_schematic_connectivity(
    project: &viamind_doc::Project,
    cursor: EventCursor,
) -> Result<SchematicConnectivityResult, CommandError> {
    calculate(capture(project, cursor)?, &SchematicConnectivityQuery { offset: 0, limit: MAX_MEMBERS }, MAX_CANDIDATES)
}
pub(crate) fn capture(project: &viamind_doc::Project, cursor: EventCursor) -> Result<Snapshot, CommandError> {
    let sheets = &project.schematic.sheets;
    if sheets.len() > 1000 {
        return Err(invalid("query supports at most 1000 sheets"));
    }
    let mut count = sheets.len();
    let mut text = 0usize;
    // Count before validation/cloning: legacy content may exceed native bounds.
    for sheet in sheets.values() {
        text = text.saturating_add(sheet.name.len());
        if let Some(c) = &sheet.content {
            let (records, bytes) = viamind_doc::symbol_package::membership_budget(c).map_err(invalid)?;
            count = count.saturating_add(c.object_count()).saturating_add(records.saturating_sub(c.packages.len()));
            text = text.saturating_add(bytes);
            if count > MAX_MEMBERS {
                return Err(invalid("query supports at most 10000 objects including sheets, symbols and pins"));
            }
            for symbol in c.symbols.values() {
                count = count.saturating_add(symbol.pins.len());
                if count > MAX_MEMBERS {
                    return Err(invalid("query supports at most 10000 objects including sheets, symbols and pins"));
                }
                text = text
                    .saturating_add(symbol.designator.len())
                    .saturating_add(symbol.value.len())
                    .saturating_add(symbol.footprint.as_ref().map_or(0, String::len));
                for pin in &symbol.pins {
                    text = text.saturating_add(pin.number.len()).saturating_add(pin.name.len());
                }
            }
            for label in c.labels.values() {
                text = text.saturating_add(label.name.len());
            }
        }
        if count > MAX_MEMBERS || text > MAX_TEXT_BYTES {
            return Err(invalid(
                "query supports at most 10000 sheets/symbols/pins/wires/junctions/labels and 2 MiB schematic text; reduce the schematic",
            ));
        }
    }
    if project.id.as_uuid().is_nil() {
        return Err(invalid("nil project identity"));
    }
    let mut ids = BTreeSet::from([project.id]);
    for (key, sheet) in sheets {
        if *key != sheet.id {
            return Err(invalid(format!("sheet key does not match identity {}", sheet.id)));
        }
        validate_sheet(sheet).map_err(|e| invalid(format!("sheet {}: {e}", sheet.id)))?;
        let mut insert = |id: ObjectId| {
            if id.as_uuid().is_nil() || !ids.insert(id) { Err(invalid(format!("nil or duplicate schematic identity {id}"))) } else { Ok(()) }
        };
        insert(sheet.id)?;
        if let Some(c) = &sheet.content {
            for id in c.packages.keys().chain(c.wires.keys()).chain(c.junctions.keys()).chain(c.labels.keys()) {
                insert(*id)?;
            }
            for s in c.symbols.values() {
                insert(s.id)?;
                for pin in &s.pins {
                    insert(pin.id)?;
                }
            }
        }
    }
    Ok(Snapshot { project_id: project.id, source_cursor: cursor, sheets: sheets.values().cloned().collect() })
}
struct Union {
    parent: Vec<usize>,
}
impl Union {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect() }
    }
    fn root(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        self.parent[a.max(b)] = a.min(b);
    }
}
fn point(c: Coordinate) -> Point {
    Point::new(c.x, c.y)
}
fn on_segment(p: Coordinate, a: Coordinate, b: Coordinate) -> bool {
    let (px, py, ax, ay, bx, by) = (i128::from(p.x.0), i128::from(p.y.0), i128::from(a.x.0), i128::from(a.y.0), i128::from(b.x.0), i128::from(b.y.0));
    (px - ax) * (by - ay) == (py - ay) * (bx - ax) && px >= ax.min(bx) && px <= ax.max(bx) && py >= ay.min(by) && py <= ay.max(by)
}
fn key(p: Coordinate) -> (i64, i64) {
    (p.x.0, p.y.0)
}
struct Node {
    member: SchematicNetMember,
    wire: Option<(Coordinate, Coordinate)>,
    name: Option<SchematicNetName>,
}
fn rank(scope: LabelScope) -> u8 {
    match scope {
        LabelScope::Global => 0,
        LabelScope::Power => 1,
        LabelScope::Local => 2,
    }
}
fn calculate(snapshot: Snapshot, query: &SchematicConnectivityQuery, candidate_limit: usize) -> Result<SchematicConnectivityResult, CommandError> {
    let mut nodes = Vec::new();
    let mut coverage = SchematicConnectivityCoverage {
        sheets: snapshot.sheets.len(),
        legacy_empty_sheets: 0,
        symbols: 0,
        symbols_without_pins: 0,
        members: 0,
        candidate_contacts: 0,
        max_objects_including_sheets_and_symbols: MAX_MEMBERS,
        max_text_bytes: MAX_TEXT_BYTES,
        max_candidate_contacts: MAX_CANDIDATES,
        exclusions: vec![
            "board nets and board annotation",
            "ERC and electrical pin compatibility",
            "buses and hierarchical sheet pins",
            "implicit hidden power-pin connections and net classes",
            "symbol bodies do not conduct; legacy empty sheets have no modeled members",
        ],
    };
    for sheet in &snapshot.sheets {
        let Some(c) = &sheet.content else {
            coverage.legacy_empty_sheets += 1;
            continue;
        };
        let units: BTreeMap<_, _> = c.packages.values().flat_map(|g| g.units.iter().map(move |u| (u.symbol_id, (g.id, u.key.as_str())))).collect();
        let mut push = |id, kind, position, wire, symbol_id: Option<ObjectId>, pin_number, electrical, name| {
            let unit = symbol_id.and_then(|id| units.get(&id));
            nodes.push(Node {
                member: SchematicNetMember {
                    package_id: unit.map(|u| u.0),
                    unit_key: unit.map(|u| u.1.to_owned()),
                    sheet_id: sheet.id,
                    id,
                    kind,
                    position,
                    symbol_id,
                    pin_number,
                    electrical,
                },
                wire,
                name,
            });
        };
        for w in c.wires.values() {
            push(w.id, SchematicMemberKind::Wire, None, Some((w.start, w.end)), None, None, None, None);
        }
        for j in c.junctions.values() {
            push(j.id, SchematicMemberKind::Junction, Some(j.position), None, None, None, None, None);
        }
        for l in c.labels.values() {
            push(
                l.id,
                SchematicMemberKind::Label,
                Some(l.position),
                None,
                None,
                None,
                None,
                Some(SchematicNetName { sheet_id: sheet.id, label_id: l.id, name: l.name.clone(), scope: l.scope }),
            );
        }
        for s in c.symbols.values() {
            coverage.symbols += 1;
            coverage.symbols_without_pins += usize::from(s.pins.is_empty());
            for p in &s.pins {
                push(
                    p.id,
                    SchematicMemberKind::Pin,
                    Some(s.pin_position(p.position).map_err(invalid)?),
                    None,
                    Some(s.id),
                    Some(p.number.clone()),
                    Some(p.electrical),
                    None,
                );
            }
        }
    }
    nodes.sort_by_key(|n| (n.member.sheet_id, n.member.id));
    coverage.members = nodes.len();
    let mut union = Union::new(nodes.len());
    let mut by_sheet: BTreeMap<ObjectId, Vec<usize>> = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        by_sheet.entry(n.member.sheet_id).or_default().push(i);
    }
    for indices in by_sheet.values() {
        let index = SpatialIndex::bulk_load(indices.iter().filter_map(|&i| nodes[i].wire.map(|(a, b)| (i, Rect::new(point(a), point(b))))));
        let mut contacts = |p: Coordinate| -> Result<Vec<usize>, CommandError> {
            let mut found = Vec::new();
            for (i, _) in index.query_point(point(p), Nm(0)) {
                coverage.candidate_contacts += 1;
                if coverage.candidate_contacts > candidate_limit {
                    return Err(invalid("exceeded 1000000 spatial candidate contacts; simplify dense/overlapping wire geometry"));
                }
                let (a, b) = nodes[i].wire.expect("indexed wires only");
                if on_segment(p, a, b) {
                    found.push(i);
                }
            }
            Ok(found)
        };
        // Coincident pins and junctions form physical point contacts; labels handled separately.
        let mut physical: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        for &i in indices {
            let n = &nodes[i];
            if matches!(n.member.kind, SchematicMemberKind::Pin | SchematicMemberKind::Junction) {
                let p = n.member.position.expect("point member");
                if let Some(&other) = physical.get(&key(p)) {
                    union.join(i, other);
                } else {
                    physical.insert(key(p), i);
                }
            }
        }
        for &i in indices {
            let n = &nodes[i];
            if let Some((a, b)) = n.wire {
                for p in [a, b] {
                    for j in contacts(p)? {
                        union.join(i, j);
                    }
                    if let Some(&j) = physical.get(&key(p)) {
                        union.join(i, j);
                    }
                }
            } else if n.member.kind == SchematicMemberKind::Junction {
                for j in contacts(n.member.position.expect("junction position"))? {
                    union.join(i, j);
                }
            }
        }
        // Resolve all anchors against the physical graph BEFORE any label attachment/name union.
        let mut anchors = Vec::new();
        for &i in indices {
            let n = &nodes[i];
            if n.member.kind != SchematicMemberKind::Label {
                continue;
            }
            let p = n.member.position.expect("label position");
            let mut roots: BTreeSet<usize> = contacts(p)?.into_iter().map(|j| union.root(j)).collect();
            if let Some(&j) = physical.get(&key(p)) {
                roots.insert(union.root(j));
            }
            if roots.len() > 1 {
                return Err(invalid(format!(
                    "ambiguous label {} on sheet {} touches separate physical components; add a junction or move the label anchor",
                    n.member.id, n.member.sheet_id
                )));
            }
            anchors.push((i, p, roots.first().copied()));
        }
        let mut label_points = BTreeMap::new();
        for (i, p, target) in anchors {
            if let Some(j) = target {
                union.join(i, j);
            }
            if let Some(&j) = label_points.get(&key(p)) {
                union.join(i, j);
            } else {
                label_points.insert(key(p), i);
            }
        }
    }
    let mut local_names = BTreeMap::new();
    let mut global_names = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if let Some(name) = &n.name {
            if let Some(&j) = local_names.get(&(name.sheet_id, name.name.as_str())) {
                union.join(i, j);
            } else {
                local_names.insert((name.sheet_id, name.name.as_str()), i);
            }
            if name.scope != LabelScope::Local {
                if let Some(&j) = global_names.get(name.name.as_str()) {
                    union.join(i, j);
                } else {
                    global_names.insert(name.name.as_str(), i);
                }
            }
        }
    }
    let mut components: BTreeMap<usize, SchematicNet> = BTreeMap::new();
    for (i, node) in nodes.into_iter().enumerate() {
        let root = union.root(i);
        let net = components.entry(root).or_insert_with(|| SchematicNet {
            id: format!("{}:{}", node.member.sheet_id, node.member.id),
            canonical_name: None,
            aliases: Vec::new(),
            multiple_names: false,
            names: Vec::new(),
            members: Vec::new(),
        });
        net.members.push(node.member);
        if let Some(name) = node.name {
            net.names.push(name);
        }
    }
    let total_nets = components.len();
    let nets = components
        .into_values()
        .skip(query.offset)
        .take(query.limit)
        .map(|mut net| {
            net.names.sort_by(|a, b| (rank(a.scope), &a.name, a.sheet_id, a.label_id).cmp(&(rank(b.scope), &b.name, b.sheet_id, b.label_id)));
            net.canonical_name = net.names.first().map(|n| n.name.clone());
            net.aliases = net.names.iter().map(|n| n.name.clone()).collect::<BTreeSet<_>>().into_iter().collect();
            net.multiple_names = net.aliases.len() > 1;
            net
        })
        .collect();
    let end = query.offset.saturating_add(query.limit);
    Ok(SchematicConnectivityResult {
        project_id: snapshot.project_id,
        source_cursor: snapshot.source_cursor,
        total_nets,
        next_offset: (end < total_nets).then_some(end),
        nets,
        coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use viamind_doc::{
        Project,
        schematic::{PinType, SchematicContent, SchematicJunction, SchematicLabel, SchematicSymbol, SchematicWire, SymbolPin},
    };
    fn p(x: i64, y: i64) -> Coordinate {
        Coordinate { x: Nm(x), y: Nm(y) }
    }
    fn wire(c: &mut SchematicContent, a: Coordinate, b: Coordinate) -> ObjectId {
        let id = ObjectId::new();
        c.wires.insert(id, SchematicWire { id, start: a, end: b });
        id
    }
    fn label(c: &mut SchematicContent, pos: Coordinate, name: &str, scope: LabelScope) -> ObjectId {
        let id = ObjectId::new();
        c.labels.insert(id, SchematicLabel { id, position: pos, name: name.into(), scope });
        id
    }
    fn junction(c: &mut SchematicContent, pos: Coordinate) {
        let id = ObjectId::new();
        c.junctions.insert(id, SchematicJunction { id, position: pos });
    }
    fn engine(contents: Vec<SchematicContent>) -> EngineHandle {
        let mut project = Project::new("connections");
        for c in contents {
            let mut sheet = Sheet::new("S");
            sheet.content = Some(c);
            project.schematic.sheets.insert(sheet.id, sheet);
        }
        EngineHandle::new(project)
    }
    fn query(engine: &EngineHandle) -> SchematicConnectivityResult {
        engine.query_schematic_connectivity(&SchematicConnectivityQuery::default()).unwrap()
    }
    fn symbol(c: &mut SchematicContent, pos: Coordinate, local: Coordinate, rotation: u8, mirrored: bool) -> ObjectId {
        let id = ObjectId::new();
        let pin = ObjectId::new();
        c.symbols.insert(
            id,
            SchematicSymbol {
                id,
                designator: format!("U{}", c.symbols.len()),
                value: String::new(),
                footprint: None,
                position: pos,
                rotation_quarters: rotation,
                mirrored,
                width: Nm(10),
                height: Nm(10),
                pins: vec![SymbolPin { id: pin, number: "1".into(), name: String::new(), position: local, electrical: PinType::PowerInput }],
            },
        );
        pin
    }
    #[test]
    fn crossing_needs_junction_but_t_and_overlap_connect() {
        let mut c = SchematicContent::default();
        wire(&mut c, p(-10, 0), p(10, 0));
        wire(&mut c, p(0, -10), p(0, 10));
        assert_eq!(query(&engine(vec![c.clone()])).total_nets, 2);
        junction(&mut c, p(0, 0));
        assert_eq!(query(&engine(vec![c])).total_nets, 1);
        let mut c = SchematicContent::default();
        wire(&mut c, p(-10, 0), p(10, 0));
        wire(&mut c, p(0, 0), p(0, 10));
        wire(&mut c, p(5, 0), p(20, 0));
        let e = engine(vec![c]);
        let before = e.session_state();
        let a = query(&e);
        let b = query(&e);
        assert_eq!(a.total_nets, 1);
        assert_eq!(a.nets[0].members.len(), 3);
        assert_eq!(a, b);
        assert_eq!(before, e.session_state());
        assert!(!e.can_undo());
    }
    #[test]
    fn diagonal_integer_contacts_and_one_nm_near_misses_at_extremes() {
        let mut c = SchematicContent::default();
        wire(&mut c, p(-10_000_000_000, -10_000_000_000), p(10_000_000_000, 10_000_000_000));
        wire(&mut c, p(0, 1), p(0, 10));
        wire(&mut c, p(0, 0), p(10, 0));
        assert_eq!(query(&engine(vec![c])).total_nets, 2);
    }
    #[test]
    fn pin_interior_requires_junction_and_mirror_quarter_transform_is_exact() {
        let mut c = SchematicContent::default();
        wire(&mut c, p(-10, 0), p(10, 0));
        symbol(&mut c, p(0, 0), p(0, 0), 0, false);
        assert_eq!(query(&engine(vec![c.clone()])).total_nets, 2);
        junction(&mut c, p(0, 0));
        assert_eq!(query(&engine(vec![c])).total_nets, 1);
        for rotation in 0..4 {
            for mirrored in [false, true] {
                let mut c = SchematicContent::default();
                let pin = symbol(&mut c, p(100, 200), p(3, 7), rotation, mirrored);
                let endpoint = c.symbols.values().next().unwrap().pin_position(p(3, 7)).unwrap();
                wire(&mut c, endpoint, p(1000, 2000));
                let r = query(&engine(vec![c]));
                assert_eq!(r.total_nets, 1);
                assert_eq!(r.nets[0].members.iter().find(|m| m.id == pin).unwrap().position, Some(endpoint));
            }
        }
        let mut c = SchematicContent::default();
        symbol(&mut c, p(5, 5), p(0, 0), 0, false);
        symbol(&mut c, p(5, 5), p(0, 0), 0, false);
        assert_eq!(query(&engine(vec![c])).total_nets, 1);
    }
    #[test]
    fn label_crossing_rejects_without_partial_result_and_junction_resolves() {
        let mut c = SchematicContent::default();
        wire(&mut c, p(-10, 0), p(10, 0));
        wire(&mut c, p(0, -10), p(0, 10));
        let id = label(&mut c, p(0, 0), "N", LabelScope::Local);
        let e = engine(vec![c.clone()]);
        let before = e.session_state();
        let error = e.query_schematic_connectivity(&SchematicConnectivityQuery::default()).unwrap_err().to_string();
        assert!(error.contains(&id.to_string()));
        assert!(error.contains("junction"));
        assert_eq!(before, e.session_state());
        junction(&mut c, p(0, 0));
        let r = query(&engine(vec![c]));
        assert_eq!(r.total_nets, 1);
        assert_eq!(r.nets[0].canonical_name.as_deref(), Some("N"));
    }
    #[test]
    fn local_global_power_scope_literal_case_alias_priority_and_isolated_members() {
        let mut a = SchematicContent::default();
        let mut b = SchematicContent::default();
        label(&mut a, p(0, 0), "N", LabelScope::Local);
        label(&mut b, p(0, 0), "N", LabelScope::Global);
        assert_eq!(query(&engine(vec![a.clone(), b.clone()])).total_nets, 2);
        label(&mut a, p(10, 0), "N", LabelScope::Power);
        label(&mut b, p(10, 0), "n", LabelScope::Global);
        let r = query(&engine(vec![a.clone(), b]));
        assert_eq!(r.total_nets, 2);
        assert_eq!(r.nets.iter().find(|n| n.canonical_name.as_deref() == Some("N")).unwrap().members.len(), 3);
        label(&mut a, p(0, 0), "Z", LabelScope::Global);
        label(&mut a, p(10, 0), "A", LabelScope::Local);
        let r = query(&engine(vec![a]));
        assert_eq!(r.total_nets, 1);
        let n = &r.nets[0];
        assert_eq!(n.canonical_name.as_deref(), Some("Z"));
        assert_eq!(n.aliases, ["A", "N", "Z"]);
        assert!(n.multiple_names);
        let mut c = SchematicContent::default();
        symbol(&mut c, p(0, 0), p(0, 0), 0, false);
        symbol(&mut c, p(10, 0), p(0, 0), 0, false);
        assert_eq!(query(&engine(vec![c])).total_nets, 2); // PowerInput alone never connects globally.
    }
    #[test]
    fn pagination_bounds_legacy_coverage_and_invalid_content_are_honest() {
        let mut c = SchematicContent::default();
        for i in 0..3 {
            wire(&mut c, p(i * 10, 0), p(i * 10 + 1, 0));
        }
        let e = engine(vec![c]);
        let all = query(&e);
        let first = e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: 0, limit: 1 }).unwrap();
        let next = e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: 1, limit: 100 }).unwrap();
        assert_eq!(first.nets, all.nets[..1]);
        assert_eq!(next.nets, all.nets[1..]);
        assert_eq!(first.next_offset, Some(1));
        assert_eq!(next.next_offset, None);
        for limit in [0, 101, usize::MAX] {
            assert!(e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: 0, limit }).is_err());
        }
        assert!(e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: usize::MAX, limit: 100 }).unwrap().nets.is_empty());
        let mut project = Project::new("legacy");
        let sheet = Sheet::new("S");
        project.schematic.sheets.insert(sheet.id, sheet);
        let r = query(&EngineHandle::new(project));
        assert_eq!(r.total_nets, 0);
        assert_eq!(r.coverage.legacy_empty_sheets, 1);
        assert!(!r.coverage.exclusions.is_empty());
        let mut c = SchematicContent::default();
        wire(&mut c, p(i64::MAX, 0), p(0, 0));
        assert!(engine(vec![c]).query_schematic_connectivity(&SchematicConnectivityQuery::default()).is_err());
        let mut c = SchematicContent::default();
        for i in 0..MAX_MEMBERS {
            junction(&mut c, p(i as i64, 0));
        }
        assert!(engine(vec![c]).query_schematic_connectivity(&SchematicConnectivityQuery::default()).unwrap_err().to_string().contains("10000"));
        let mut project = Project::new("text");
        let sheet = Sheet::new("x".repeat(MAX_TEXT_BYTES + 1));
        project.schematic.sheets.insert(sheet.id, sheet);
        assert!(
            EngineHandle::new(project)
                .query_schematic_connectivity(&SchematicConnectivityQuery::default())
                .unwrap_err()
                .to_string()
                .contains("2 MiB")
        );
    }
    #[test]
    fn sparse_thousand_wire_board_independent_query_uses_broadphase() {
        let mut c = SchematicContent::default();
        for i in 0..1000 {
            wire(&mut c, p(i * 100, 0), p(i * 100 + 10, 0));
        }
        let e = engine(vec![c]);
        let start = std::time::Instant::now();
        let result = query(&e);
        eprintln!("T61 1000 sparse wires query: {:.3} ms; {} candidates", start.elapsed().as_secs_f64() * 1000.0, result.coverage.candidate_contacts);
        assert_eq!(result.total_nets, 1000);
        assert_eq!(result.coverage.candidate_contacts, 2000);
        assert_eq!(result.next_offset, Some(100));
        assert!(result.nets.windows(2).all(|pair| pair[0].id < pair[1].id));
    }
    #[test]
    fn spatial_budget_rejects_dense_candidates_without_partial_results() {
        let mut c = SchematicContent::default();
        for _ in 0..10 {
            wire(&mut c, p(0, 0), p(100, 100));
        }
        let mut sheet = Sheet::new("dense");
        sheet.content = Some(c);
        let e = engine(vec![]);
        let snapshot = Snapshot { project_id: e.read(|p| p.id), source_cursor: e.with_engine(crate::Engine::event_cursor), sheets: vec![sheet] };
        assert!(calculate(snapshot, &SchematicConnectivityQuery::default(), 20).unwrap_err().to_string().contains("candidate"));
    }
}
