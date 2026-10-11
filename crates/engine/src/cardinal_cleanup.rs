//! Equivalent straight cardinal copper cleanup, with immutable shared joint preflight.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{CopperLayer, ObjectId, Project, Track};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};

use crate::{Command, CommandError, Delta, EngineHandle, EventCursor};

const MAX_SELECTED: usize = 2048;
const MAX_DELETED: usize = 1024;
const MAX_HOLE_CANDIDATES: usize = 1_000_000;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Permit {
    fn acquire() -> Result<Self, CommandError> {
        let mut n = ACTIVE.load(Ordering::Acquire);
        loop {
            if n >= 2 {
                return Err(CommandError::conflict("track cleanup busy; two operations already retained"));
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
        ACTIVE.fetch_sub(1, Ordering::Release);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum CleanupTracks {
    Ids { ids: Vec<ObjectId> },
    Net { net: String, layer: Option<CopperLayer> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanupCardinalTracks {
    pub expected_project: ObjectId,
    pub expected_cursor: EventCursor,
    pub tracks: CleanupTracks,
    #[serde(default)]
    pub rules: viamind_drc::DrcInput,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CleanupCounts {
    pub selected_tracks: usize,
    pub unsupported_diagonal_tracks: usize,
    pub unsupported_unassigned_tracks: usize,
    pub retained_anchor_joints: usize,
    pub retained_branch_joints: usize,
    pub retained_mismatch_joints: usize,
    pub retained_hole_chains: usize,
    pub retained_duplicate_chains: usize,
    pub merged_chains: usize,
    pub deleted_tracks: usize,
    pub hole_candidates: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CardinalTrackReplacement {
    pub before: Vec<Track>,
    pub after: Track,
}
#[derive(Clone, Debug, Serialize)]
pub struct CheckedCardinalCleanup {
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub document_net_rules: viamind_doc::NamedNetRules,
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    pub counts: CleanupCounts,
    pub replacements: Vec<CardinalTrackReplacement>,
    /// Actual bounded checker coverage, including exclusions and unperformed checks.
    pub coverage: viamind_drc::Coverage,
    pub existing_unrelated_violations: usize,
    pub complete_scoped_preflight: bool,
}
impl Command for CleanupCardinalTracks {
    const NAME: &'static str = "cleanup_cardinal_tracks";
    const DESCRIPTION: &'static str = "Recompute and atomically merge equivalent cardinal straight-track joints from explicit IDs or an exact net/optional outer layer. Requires expected_project/full initial expected_cursor. Same assigned net, side and exact width; no overlap/reversal/diagonal or bend optimization. Preserve all branch and exact quarter-turn explicit pad/via anchors; any chain touching ANY via drill-hole AABB is retained conservatively to preserve existing guide proofs. Deterministic smallest survivor ID. Bounded100k board structural objects/2MiB text/2048 selected tracks/1024 deletions/1M hole candidates; two retained operations. Complete joint scoped DRC of replacements required; incomplete/truncated or affected violations refuse, unrelated violations/checker exclusions may remain. One undo/redo, no direct physical connectivity certification. Use plan_cardinal_track_cleanup for immutable checked preview.";
    fn expected_cursor(&self) -> Option<EventCursor> {
        Some(self.expected_cursor)
    }
    fn apply(self, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
        let _permit = Permit::acquire()?;
        let (_, deltas) = prepare(project, &self)?;
        for delta in &deltas {
            delta.apply_to(project);
        }
        Ok(deltas)
    }
}
impl EngineHandle {
    pub fn plan_cardinal_track_cleanup(&self, input: &CleanupCardinalTracks) -> Result<CheckedCardinalCleanup, CommandError> {
        crate::command_hooks::check_read().map_err(|e| crate::CommandError::Invalid { field: "command_hooks".into(), message: e.to_string() })?;
        let _permit = Permit::acquire()?;
        let project = self.with_engine(|e| {
            if e.event_cursor() != input.expected_cursor || e.project().id != input.expected_project {
                return Err(CommandError::conflict("cleanup session changed; refresh project and full cursor"));
            }
            capture(e.project())?;
            // Selector bounds precede owned board/ID capture, including net matches.
            selected(e.project(), &input.tracks)?;
            Ok(Project { id: e.project().id, name: String::new(), schematic: Default::default(), board: e.project().board.clone_without_assembly() })
        })?;
        prepare(&project, input).map(|(plan, _)| plan)
    }
}
fn invalid(message: impl Into<String>) -> CommandError {
    CommandError::invalid("cleanup", message)
}
fn capture(project: &Project) -> Result<(), CommandError> {
    crate::auto_drc::snapshot_budget(project, 100_000).map_err(invalid)
}
fn selected(project: &Project, selector: &CleanupTracks) -> Result<BTreeSet<ObjectId>, CommandError> {
    match selector {
        CleanupTracks::Ids { ids } => {
            if ids.is_empty() || ids.len() > MAX_SELECTED {
                return Err(invalid("supply1..2048 distinct stored track IDs"));
            }
            let mut result = BTreeSet::new();
            for id in ids {
                if !project.board.tracks.contains_key(id) {
                    return Err(CommandError::not_found("track", *id));
                }
                if !result.insert(*id) {
                    return Err(invalid("duplicate track ID"));
                }
            }
            Ok(result)
        }
        CleanupTracks::Net { net, layer } => {
            if net.is_empty() || net.len() > 512 || net.trim() != net || net.chars().count() > 128 || net.chars().any(char::is_control) {
                return Err(invalid("exact trimmed nonempty net name of at most128 characters required"));
            }
            let matches = || project.board.tracks.values().filter(|t| t.net.as_ref() == Some(net) && layer.is_none_or(|l| t.layer == l));
            if matches().take(MAX_SELECTED + 1).count() > MAX_SELECTED {
                return Err(invalid("net selector exceeds2048 tracks"));
            }
            Ok(matches().map(|t| t.id).collect())
        }
    }
}
type Position = (bool, i64, i64);
fn position(layer: CopperLayer, p: Point) -> Position {
    (layer == CopperLayer::Bottom, p.x.0, p.y.0)
}
fn other(track: &Track, p: Point) -> Point {
    if track.start == p { track.end } else { track.start }
}
fn cardinal(t: &Track) -> bool {
    t.start.x == t.end.x || t.start.y == t.end.y
}
fn ordered(a: Point, b: Point) -> (Point, Point) {
    if (a.x.0, a.y.0) <= (b.x.0, b.y.0) { (a, b) } else { (b, a) }
}
type Geometry<'a> = (Position, i64, i64, i64, Option<&'a str>);
fn geometry(t: &Track) -> Geometry<'_> {
    let (a, b) = ordered(t.start, t.end);
    (position(t.layer, a), b.x.0, b.y.0, t.width.0, t.net.as_deref())
}
fn validate_identity(ids: &mut BTreeSet<ObjectId>, key: ObjectId, id: ObjectId) -> Result<(), CommandError> {
    if key != id || id.as_uuid().is_nil() || !ids.insert(id) {
        return Err(invalid("board identities must be nonnil, unique and match map keys"));
    }
    Ok(())
}
fn prepare(project: &Project, input: &CleanupCardinalTracks) -> Result<(CheckedCardinalCleanup, Vec<Delta>), CommandError> {
    if project.id != input.expected_project {
        return Err(CommandError::conflict("cleanup project changed"));
    }
    capture(project)?;
    input.rules.validate().map_err(|e| invalid(e.to_string()))?;
    let selected = selected(project, &input.tracks)?;
    let mut counts = CleanupCounts { selected_tracks: selected.len(), ..Default::default() };
    let mut identities = BTreeSet::new();
    validate_identity(&mut identities, project.id, project.id)?;
    let mut anchors = BTreeSet::new();
    for (key, part) in &project.board.parts {
        validate_identity(&mut identities, *key, part.id)?;
        if let Some(pads) = &part.pads {
            for pad in &pads.0 {
                validate_identity(&mut identities, pad.id, pad.id)?;
                let center = part.local_to_world_nm_exact(pad.position).map_err(invalid)?;
                let layer = if part.side == viamind_doc::Side::Bottom { CopperLayer::Bottom } else { CopperLayer::Top };
                anchors.insert(position(layer, center));
            }
        }
    }
    for (key, via) in &project.board.vias {
        validate_identity(&mut identities, *key, via.id)?;
        via.validate().map_err(invalid)?;
        anchors.insert(position(CopperLayer::Top, via.position));
        anchors.insert(position(CopperLayer::Bottom, via.position));
    }
    let holes = SpatialIndex::bulk_load(project.board.vias.values().map(|v| (v.id, Rect::from_point(v.position).inflate(Nm((v.drill.0 + 1) / 2)))));
    // The capture bounds all endpoint/index allocation before this point. Borrow
    // geometry keys and net strings rather than cloning global labels.
    let mut endpoints: BTreeMap<Position, Vec<ObjectId>> = BTreeMap::new();
    let mut geometries: BTreeSet<Geometry<'_>> = BTreeSet::new();
    for (key, t) in &project.board.tracks {
        validate_identity(&mut identities, *key, t.id)?;
        t.validate().map_err(invalid)?;
        endpoints.entry(position(t.layer, t.start)).or_default().push(t.id);
        endpoints.entry(position(t.layer, t.end)).or_default().push(t.id);
        geometries.insert(geometry(t));
    }
    let mut eligible = BTreeSet::new();
    for id in &selected {
        let t = &project.board.tracks[id];
        if !cardinal(t) {
            counts.unsupported_diagonal_tracks += 1;
        } else if t.net.is_none() {
            counts.unsupported_unassigned_tracks += 1;
        } else {
            eligible.insert(*id);
        }
    }
    let mut neighbors: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    for (&joint, tracks) in &endpoints {
        if !tracks.iter().any(|id| eligible.contains(id)) {
            continue;
        }
        if anchors.contains(&joint) {
            counts.retained_anchor_joints += 1;
            continue;
        }
        if tracks.len() != 2 {
            if tracks.len() > 2 {
                counts.retained_branch_joints += 1;
            }
            continue;
        }
        let (a, b) = (&project.board.tracks[&tracks[0]], &project.board.tracks[&tracks[1]]);
        let p = Point::new(Nm(joint.1), Nm(joint.2));
        let (aa, bb) = (other(a, p), other(b, p));
        let opposite = (aa.x == p.x && bb.x == p.x && (aa.y < p.y) != (bb.y < p.y)) || (aa.y == p.y && bb.y == p.y && (aa.x < p.x) != (bb.x < p.x));
        if !eligible.contains(&a.id) || !eligible.contains(&b.id) || a.width != b.width || a.net != b.net || !opposite {
            counts.retained_mismatch_joints += 1;
            continue;
        }
        neighbors.entry(a.id).or_default().push(b.id);
        neighbors.entry(b.id).or_default().push(a.id);
    }
    let mut visited = BTreeSet::new();
    let mut replacements = Vec::new();
    for id in eligible {
        if !visited.insert(id) {
            continue;
        }
        let mut chain = BTreeSet::from([id]);
        let mut pending = vec![id];
        while let Some(current) = pending.pop() {
            for next in neighbors.get(&current).into_iter().flatten() {
                if visited.insert(*next) {
                    chain.insert(*next);
                    pending.push(*next);
                }
            }
        }
        if chain.len() < 2 {
            continue;
        }
        let mut after = project.board.tracks[chain.first().expect("nonempty chain")].clone();
        let bounds = Rect::from_points(chain.iter().flat_map(|id| {
            let t = &project.board.tracks[id];
            [t.start, t.end]
        }))
        .expect("nonempty chain");
        (after.start, after.end) = (bounds.min, bounds.max);
        let mut near_hole = false;
        for _ in holes.query_rect(&bounds) {
            if counts.hole_candidates >= MAX_HOLE_CANDIDATES {
                return Err(invalid("cleanup hole candidate limit1000000 exceeded"));
            }
            counts.hole_candidates += 1;
            near_hole = true;
        }
        if near_hole {
            counts.retained_hole_chains += 1;
            continue;
        }
        // Every original chain segment is strictly shorter than its replacement,
        // so any existing equal geometry necessarily belongs outside this chain.
        if geometries.contains(&geometry(&after)) {
            counts.retained_duplicate_chains += 1;
            continue;
        }
        if counts.deleted_tracks.saturating_add(chain.len() - 1) > MAX_DELETED {
            return Err(invalid("cleanup deletion limit1024 exceeded"));
        }
        counts.deleted_tracks += chain.len() - 1;
        counts.merged_chains += 1;
        replacements.push(CardinalTrackReplacement { before: chain.into_iter().map(|id| project.board.tracks[&id].clone()).collect(), after });
    }
    replacements.sort_by_key(|r| r.after.id);
    let mut deltas = Vec::new();
    let mut affected = BTreeSet::new();
    for replacement in &replacements {
        affected.insert(replacement.after.id);
        for t in &replacement.before {
            deltas.push(Delta::Track { id: t.id, before: Some(t.clone()), after: (t.id == replacement.after.id).then(|| replacement.after.clone()) });
        }
    }
    let outline = project.board.outline.as_ref().ok_or_else(|| invalid("explicit substrate outline required for complete scoped cleanup DRC"))?;
    outline.validate().map_err(|e| invalid(e.to_string()))?;
    let mut candidate = Project { id: project.id, name: String::new(), schematic: Default::default(), board: project.board.clone_without_assembly() };
    for delta in &deltas {
        delta.apply_to(&mut candidate);
    }
    let report = viamind_drc::run_drc(&candidate, &input.rules).map_err(|e| invalid(e.to_string()))?;
    if report.truncated || report.incomplete_reason.is_some() {
        return Err(CommandError::conflict(format!(
            "cleanup preflight incomplete; no copper changed: {}",
            report.incomplete_reason.as_deref().unwrap_or("DRC result truncated")
        )));
    }
    if let Some(v) = report.violations.iter().find(|v| v.objects.iter().any(|o| affected.contains(&o.selection_id()))) {
        return Err(CommandError::conflict(format!("cleanup preflight failed; no copper changed: {}", v.message)));
    }
    Ok((
        CheckedCardinalCleanup {
            document_net_rules: project.board.net_rules.clone(),
            project_id: project.id,
            source_cursor: input.expected_cursor,
            counts,
            replacements,
            coverage: report.coverage,
            existing_unrelated_violations: report.violations.len(),
            complete_scoped_preflight: true,
        },
        deltas,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn third_retained_operation_refuses_and_dropped_admission_recovers() {
        let first = Permit::acquire().unwrap();
        let second = Permit::acquire().unwrap();
        assert!(Permit::acquire().is_err());
        drop(first);
        let replacement = Permit::acquire().unwrap();
        assert!(Permit::acquire().is_err());
        drop((second, replacement));
        assert!(Permit::acquire().is_ok());
    }
}
