//! Read-only explicit-pad membership and conservative remaining connection guides.
use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use viamind_doc::{ObjectId, Project};

use crate::CommandError;
use crate::copper_connectivity::{CopperProof, Forest};

const fn default_limit() -> usize {
    100
}
const EXACT_LIMIT: usize = 256;

/// Filters and pagination for case-sensitive named-net summaries.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetQuery {
    /// Exclusive net-name cursor; repeat only while the design is unchanged.
    pub after: Option<String>,
    /// Page size, 1–1000; default100.
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: usize,
}
/// One named net. Guides use conservative exact-endpoint copper proofs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NetSummary {
    pub name: String,
    pub terminals: usize,
    pub guide_count: usize,
}
/// Bounded net summary page in case-sensitive name order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NetQueryResult {
    pub copper_proof: crate::CopperConnectivityCoverage,
    pub nets: Vec<NetSummary>,
    pub total: usize,
    pub next_after: Option<String>,
}

/// A terminal's identity and transformed pad centre, in millimetres at the query boundary.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Terminal {
    pub part_id: ObjectId,
    pub pad_id: ObjectId,
    pub designator: String,
    pub number: String,
    pub x_mm: f64,
    pub y_mm: f64,
}
/// A required centre-to-centre guide, never a copper object or DRC result.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConnectionGuide {
    pub net: String,
    pub from: Terminal,
    pub to: Terminal,
    pub length_mm: f64,
}
/// Exact named-net membership and guide pagination. Arrays share the offset and limit.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectivityQuery {
    /// Exact, case-sensitive net name.
    pub net: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: usize,
}
/// One consistent snapshot. Terminal order is (part UUID, pad UUID); guides are deterministic.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConnectivityResult {
    pub copper_proof: crate::CopperConnectivityCoverage,
    pub net: String,
    /// dense_prim for <=256 terminals; coordinate_chain is an approximation for larger nets.
    pub algorithm: &'static str,
    pub terminal_count: usize,
    pub guide_count: usize,
    pub terminals: Vec<Terminal>,
    pub guides: Vec<ConnectionGuide>,
    pub next_offset: Option<usize>,
}

fn validate_limit(limit: usize) -> Result<(), CommandError> {
    if !(1..=1000).contains(&limit) {
        return Err(CommandError::invalid("limit", "must be between 1 and 1000"));
    }
    Ok(())
}
fn terminals(project: &Project, filter: Option<&str>) -> BTreeMap<String, Vec<Terminal>> {
    let mut nets: BTreeMap<String, Vec<Terminal>> = BTreeMap::new();
    for part in project.board.parts.values() {
        if let Some(pads) = &part.pads {
            for pad in &pads.0 {
                if let Some(net) = &pad.net {
                    if filter.is_some_and(|filter| filter != net) {
                        continue;
                    }
                    let (x_mm, y_mm) = part.local_to_world_mm(pad.position.x.to_mm(), pad.position.y.to_mm());
                    nets.entry(net.clone()).or_default().push(Terminal {
                        part_id: part.id,
                        pad_id: pad.id,
                        designator: part.designator.clone(),
                        number: pad.number.clone(),
                        x_mm,
                        y_mm,
                    });
                }
            }
        }
    }
    for members in nets.values_mut() {
        members.sort_by_key(|m| (m.part_id, m.pad_id));
    }
    nets
}
impl NetQuery {
    pub fn run(&self, project: &Project) -> Result<NetQueryResult, CommandError> {
        validate_limit(self.limit)?;
        let proof = CopperProof::analyze(project);
        // Count islands with borrowed names; a summary never clones every terminal.
        let mut counts: BTreeMap<&str, (usize, usize, BTreeSet<usize>)> = BTreeMap::new();
        for part in project.board.parts.values() {
            if let Some(pads) = &part.pads {
                for pad in &pads.0 {
                    if let Some(net) = pad.net.as_deref() {
                        let (terminals, unbound, roots) = counts.entry(net).or_default();
                        *terminals += 1;
                        let id = crate::PadEndpointId { part_id: part.id, pad_id: pad.id };
                        if let Some(&root) = proof.pads.get(&id) {
                            roots.insert(root);
                        } else {
                            *unbound += 1;
                        }
                    }
                }
            }
        }
        let total = counts.len();
        let mut matching = counts.into_iter().filter(|(name, _)| self.after.as_deref().is_none_or(|after| *name > after));
        let nets: Vec<_> = matching
            .by_ref()
            .take(self.limit)
            .map(|(name, (terminals, unbound, roots))| NetSummary {
                name: name.into(),
                terminals,
                guide_count: (unbound + roots.len()).saturating_sub(1),
            })
            .collect();
        let next_after = matching.next().and_then(|_| nets.last().map(|n| n.name.clone()));
        Ok(NetQueryResult { copper_proof: proof.coverage, nets, total, next_after })
    }
}
impl ConnectivityQuery {
    pub fn run(&self, project: &Project) -> Result<ConnectivityResult, CommandError> {
        validate_limit(self.limit)?;
        let mut members = terminals(project, Some(&self.net)).remove(&self.net).unwrap_or_default();
        let terminal_count = members.len();
        let algorithm = if terminal_count <= EXACT_LIMIT { "dense_prim" } else { "coordinate_chain" };
        let proof = CopperProof::analyze(project);
        let edges = remaining_tree(&members, &proof);
        let guide_count = edges.len();
        let guides = edges.into_iter().skip(self.offset).take(self.limit).map(|(a, b)| guide(&self.net, &members[a], &members[b])).collect();
        let end = self.offset.saturating_add(self.limit);
        let next_offset = (end < terminal_count).then_some(end);
        let terminals = members.drain(..).skip(self.offset).take(self.limit).collect();
        Ok(ConnectivityResult {
            copper_proof: proof.coverage,
            net: self.net.clone(),
            algorithm,
            terminal_count,
            guide_count,
            terminals,
            guides,
            next_offset,
        })
    }
}
/// All live guides; O(N) storage and O(N logN) time apart from bounded small-net exact trees.
/// Legacy preview pads and pads without a net never enter this result.
pub fn connection_guides(project: &Project) -> Vec<ConnectionGuide> {
    let proof = CopperProof::analyze(project);
    terminals(project, None)
        .into_iter()
        .flat_map(|(net, members)| remaining_tree(&members, &proof).into_iter().map(move |(a, b)| guide(&net, &members[a], &members[b])))
        .collect()
}
/// Same deterministic trees as visible guides, without cloning per-edge labels.
pub(crate) fn connection_guide_ids(project: &Project) -> Vec<[crate::PadEndpointId; 2]> {
    let proof = CopperProof::analyze(project);
    terminals(project, None)
        .into_values()
        .flat_map(|members| {
            remaining_tree(&members, &proof).into_iter().map(move |(a, b)| {
                let mut pair = [a, b].map(|i| crate::PadEndpointId { part_id: members[i].part_id, pad_id: members[i].pad_id });
                pair.sort();
                pair
            })
        })
        .collect()
}
fn remaining_tree(members: &[Terminal], proof: &CopperProof) -> Vec<(usize, usize)> {
    let mut forest = Forest::with_size(members.len());
    let mut islands = BTreeMap::new();
    for (i, terminal) in members.iter().enumerate() {
        let id = crate::PadEndpointId { part_id: terminal.part_id, pad_id: terminal.pad_id };
        if let Some(root) = proof.pads.get(&id)
            && let Some(previous) = islands.insert(root, i)
        {
            forest.join(previous, i);
        }
    }
    tree(members).into_iter().filter(|&(a, b)| forest.join(a, b)).collect()
}
fn distance2(a: &Terminal, b: &Terminal) -> f64 {
    (a.x_mm - b.x_mm).powi(2) + (a.y_mm - b.y_mm).powi(2)
}
fn guide(net: &str, a: &Terminal, b: &Terminal) -> ConnectionGuide {
    ConnectionGuide { net: net.into(), from: a.clone(), to: b.clone(), length_mm: distance2(a, b).sqrt() }
}
fn tree(members: &[Terminal]) -> Vec<(usize, usize)> {
    let n = members.len();
    if n < 2 {
        return Vec::new();
    }
    if n > EXACT_LIMIT {
        let chain = |axis: bool| {
            let mut indices: Vec<_> = (0..n).collect();
            indices.sort_by(|&a, &b| {
                let (a, b) = (&members[a], &members[b]);
                let (ax, ay, bx, by) = if axis { (a.x_mm, a.y_mm, b.x_mm, b.y_mm) } else { (a.y_mm, a.x_mm, b.y_mm, b.x_mm) };
                ax.total_cmp(&bx).then(ay.total_cmp(&by)).then((a.part_id, a.pad_id).cmp(&(b.part_id, b.pad_id)))
            });
            let edges: Vec<_> = indices.windows(2).map(|w| (w[0], w[1])).collect();
            let length: f64 = edges.iter().map(|&(a, b)| distance2(&members[a], &members[b]).sqrt()).sum();
            (edges, length)
        };
        let x = chain(true);
        let y = chain(false);
        return if x.1 <= y.1 { x.0 } else { y.0 };
    }
    let mut selected = vec![false; n];
    let mut best = vec![f64::INFINITY; n];
    let mut parent = vec![0; n];
    best[0] = 0.0;
    let mut edges = Vec::with_capacity(n - 1);
    for _ in 0..n {
        let next =
            (0..n).filter(|&i| !selected[i]).min_by(|&a, &b| best[a].total_cmp(&best[b]).then(a.cmp(&b))).expect("an unselected terminal remains");
        selected[next] = true;
        if next != 0 {
            edges.push((parent[next], next));
        }
        for i in 0..n {
            if !selected[i] {
                let d = distance2(&members[next], &members[i]);
                if d < best[i] || (d == best[i] && next < parent[i]) {
                    best[i] = d;
                    parent[i] = next;
                }
            }
        }
    }
    edges
}
