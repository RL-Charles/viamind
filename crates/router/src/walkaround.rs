//! Bounded static cardinal search around closed conservative obstacle boxes.
//! Geometry construction and final scoped DRC belong to the engine boundary.
use std::{cmp::Reverse, collections::BinaryHeap};

use serde::Serialize;
use viamind_geom::{Nm, Point, Rect};

use crate::{CornerMode, PadPathPlan, Posture, plan_pad_path, snap};

pub const MAX_WALKAROUND_OBSTACLES: usize = 64;
pub const MAX_WALKAROUND_NODES: usize = 20_000;
pub const MAX_WALKAROUND_EDGES: usize = 100_000;
pub const MAX_WALKAROUND_INTERSECTIONS: usize = 4_000_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WalkaroundWork {
    pub obstacle_boxes: usize,
    pub graph_nodes: usize,
    pub candidate_edges: usize,
    pub intersection_tests: usize,
    pub settled_nodes: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WalkaroundPlan {
    pub path: PadPathPlan,
    pub waypoints: Vec<Point>,
    pub work: WalkaroundWork,
}
/// One cumulative work allowance, which may cover multiple legs of one route.
#[derive(Clone, Debug, Default)]
pub struct WalkaroundBudget {
    pub used: WalkaroundWork,
    interior_waypoints: usize,
}
fn clear(bounds: Rect, obstacles: &[Rect], work: &mut WalkaroundWork, budget: &mut WalkaroundBudget) -> Result<bool, &'static str> {
    for obstacle in obstacles {
        if budget.used.intersection_tests == MAX_WALKAROUND_INTERSECTIONS {
            return Err("walkaround intersection-work limit reached; use a simpler scene or manual route");
        }
        work.intersection_tests += 1;
        budget.used.intersection_tests += 1;
        if bounds.intersects(obstacle) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn floor_grid(value: i128, grid: Nm) -> Result<i64, &'static str> {
    i64::try_from(value.div_euclid(i128::from(grid.0)) * i128::from(grid.0)).map_err(|_| "walkaround grid coordinate overflow")
}
fn ceil_grid(value: i128, grid: Nm) -> Result<i64, &'static str> {
    i64::try_from(-(-value).div_euclid(i128::from(grid.0)) * i128::from(grid.0)).map_err(|_| "walkaround grid coordinate overflow")
}
fn axis(first: Nm, last: Nm, min: Nm, max: Nm, grid: Nm, boxes: impl Iterator<Item = (Nm, Nm)>) -> Result<Vec<i64>, &'static str> {
    let mut values = vec![first.0, last.0];
    let mut add = |value: i64| {
        if min.0 <= value && value <= max.0 {
            values.push(value);
        }
    };
    add(ceil_grid(i128::from(min.0), grid)?);
    add(floor_grid(i128::from(max.0), grid)?);
    for (lo, hi) in boxes {
        add(floor_grid(i128::from(lo.0) - 1, grid)?);
        add(ceil_grid(i128::from(hi.0) + 1, grid)?);
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}
/// Search a finite coordinate graph inside an already inset rectangular domain.
/// Every obstacle is a closed clearance box. Success here still requires engine DRC.
pub fn plan_walkaround(start: Point, target: Point, grid: Nm, domain: Rect, obstacles: &[Rect]) -> Result<WalkaroundPlan, &'static str> {
    plan_walkaround_with_budget(start, target, grid, domain, obstacles, &mut WalkaroundBudget::default())
}
/// Charge cumulative counters before graph allocation and each intersection.
/// A failed leg consumes work already performed; callers must discard its plan.
pub fn plan_walkaround_with_budget(
    start: Point,
    target: Point,
    grid: Nm,
    domain: Rect,
    obstacles: &[Rect],
    budget: &mut WalkaroundBudget,
) -> Result<WalkaroundPlan, &'static str> {
    // Reuse endpoint/grid/range validation before graph allocation.
    plan_pad_path(start, target, &[], grid, CornerMode::Cardinal, Posture::Primary)?;
    if domain.min.x >= domain.max.x || domain.min.y >= domain.max.y {
        return Err("walkaround inset board domain is empty");
    }
    if [domain.min, domain.max].into_iter().any(|p| p.x.0.unsigned_abs() > 10_000_000_000 || p.y.0.unsigned_abs() > 10_000_000_000) {
        return Err("walkaround board domain exceeds ±10000 mm");
    }
    if obstacles.len().saturating_add(budget.used.obstacle_boxes) > MAX_WALKAROUND_OBSTACLES {
        return Err("walkaround supports at most64 relevant obstacle boxes; use manual routing");
    }
    if obstacles.iter().any(|r| r.min.x > r.max.x || r.min.y > r.max.y || !domain.contains_rect(r)) {
        return Err("walkaround obstacles must be valid boxes clipped to the inset board");
    }
    let first = snap(start, grid)?;
    let last = snap(target, grid)?;
    if [start, first, target, last].into_iter().any(|p| !domain.contains(p)) {
        return Err("exact endpoint or nearest grid anchor is outside the inset board; choose a smaller grid or manual route");
    }
    let mut work = WalkaroundWork { obstacle_boxes: obstacles.len(), ..Default::default() };
    budget.used.obstacle_boxes += obstacles.len();
    if !clear(Rect::new(start, first), obstacles, &mut work, budget)? || !clear(Rect::new(last, target), obstacles, &mut work, budget)? {
        return Err("exact endpoint stub touches a conservative obstacle; choose a smaller grid or manual route");
    }
    let xs = axis(first.x, last.x, domain.min.x, domain.max.x, grid, obstacles.iter().map(|r| (r.min.x, r.max.x)))?;
    let ys = axis(first.y, last.y, domain.min.y, domain.max.y, grid, obstacles.iter().map(|r| (r.min.y, r.max.y)))?;
    let nx = xs.len();
    let ny = ys.len();
    work.graph_nodes = nx.checked_mul(ny).ok_or("walkaround graph size overflow")?;
    work.candidate_edges = nx.saturating_sub(1).saturating_mul(ny).saturating_add(ny.saturating_sub(1).saturating_mul(nx));
    if work.graph_nodes.saturating_add(budget.used.graph_nodes) > MAX_WALKAROUND_NODES
        || work.candidate_edges.saturating_add(budget.used.candidate_edges) > MAX_WALKAROUND_EDGES
    {
        return Err("walkaround graph node/edge budget exceeded; use a simpler scene or manual route");
    }
    budget.used.graph_nodes += work.graph_nodes;
    budget.used.candidate_edges += work.candidate_edges;
    let point = |i: usize| Point::new(Nm(xs[i % nx]), Nm(ys[i / nx]));
    let index = |p: Point| -> Result<usize, &'static str> {
        let x = xs.binary_search(&p.x.0).map_err(|_| "walkaround anchor missing from graph")?;
        let y = ys.binary_search(&p.y.0).map_err(|_| "walkaround anchor missing from graph")?;
        Ok(y * nx + x)
    };
    let source = index(first)?;
    let finish = index(last)?;
    let mut valid = Vec::with_capacity(work.graph_nodes);
    for i in 0..work.graph_nodes {
        valid.push(clear(Rect::from_point(point(i)), obstacles, &mut work, budget)?);
    }
    let mut distances = vec![u64::MAX; work.graph_nodes];
    let mut previous = vec![None; work.graph_nodes];
    distances[source] = 0;
    let mut queue = BinaryHeap::from([Reverse((0u64, source))]);
    while let Some(Reverse((distance, i))) = queue.pop() {
        if distance != distances[i] {
            continue;
        }
        work.settled_nodes += 1;
        budget.used.settled_nodes += 1;
        if i == finish {
            break;
        }
        let x = i % nx;
        let y = i / nx;
        let neighbors =
            [x.checked_sub(1).map(|_| i - 1), (x + 1 < nx).then_some(i + 1), y.checked_sub(1).map(|_| i - nx), (y + 1 < ny).then_some(i + nx)];
        for next in neighbors.into_iter().flatten() {
            if !valid[next] || !clear(Rect::new(point(i), point(next)), obstacles, &mut work, budget)? {
                continue;
            }
            let a = point(i);
            let b = point(next);
            let length = u64::try_from((i128::from(a.x.0) - i128::from(b.x.0)).abs() + (i128::from(a.y.0) - i128::from(b.y.0)).abs())
                .map_err(|_| "walkaround edge length overflow")?;
            let proposed = distance.checked_add(length).ok_or("walkaround path length overflow")?;
            if proposed < distances[next] {
                distances[next] = proposed;
                previous[next] = Some(i);
                queue.push(Reverse((proposed, next)));
            }
        }
    }
    if distances[finish] == u64::MAX {
        return Err("no static cardinal path through conservative clearance boxes; use manual routing or change geometry");
    }
    let mut indices = vec![finish];
    while let Some(prev) = previous[*indices.last().ok_or("walkaround path reconstruction failed")?] {
        indices.push(prev);
    }
    indices.reverse();
    let mut path = Vec::<Point>::new();
    for next in indices.into_iter().map(point) {
        while path.len() >= 2 {
            let a = path[path.len() - 2];
            let b = path[path.len() - 1];
            if ((a.x == b.x && b.x == next.x) || (a.y == b.y && b.y == next.y)) && Rect::new(a, next).contains(b) {
                path.pop();
            } else {
                break;
            }
        }
        path.push(next);
    }
    let interior = path.len().saturating_sub(2);
    if interior.saturating_add(budget.interior_waypoints) > 510 {
        return Err("walkaround path exceeds510 interior waypoints; use a simpler scene or manual route");
    }
    budget.interior_waypoints += interior;
    let waypoints = if path.len() > 2 { path[1..path.len() - 1].to_vec() } else { Vec::new() };
    let path = plan_pad_path(start, target, &waypoints, grid, CornerMode::Cardinal, Posture::Primary)?;
    Ok(WalkaroundPlan { path, waypoints, work })
}
