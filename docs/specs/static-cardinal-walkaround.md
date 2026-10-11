# Bounded static cardinal obstacle walkaround

## Public sources and independent scope

- [Public PCB editor routing manual, Routing tracks](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#routing-tracks), consulted 2026-10-11: obstacle walkaround leaves existing objects stationary; routing respects configured width/clearance and refuses unsafe placement.
- [Rust BinaryHeap documentation](https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html), consulted 2026-10-11: standard priority queues and reversed ordering support deterministic shortest-path search.

These sources establish public behavior and standard APIs only. All geometry,
budgets, graph construction, UI behavior and code below are independent Viamind
choices. No external implementation, icons, assets or proprietary inspection is
used. Existing manual/cardinal/45/via commands remain available.

This increment plans a static **horizontal/vertical** route between exact
same-side, same-net, distinct quarter-turn explicit pad centers. Only an
axis-aligned rectangular four-vertex outer board with no cutouts is supported.
Other outlines, unsupported relevant pad transforms, unknown route-side legacy
pad geometry, malformed objects, exceeded budgets and unavailable paths produce
actionable errors. No automatic fallback silently substitutes a manual route.

## Exact geometry and obstacle capture

Model coordinates, bounds, grid and width are integer nanometres. Existing DRC
inputs are validated and converted once at their millimetre API boundary using
the same rules as final preflight. Grid/width/coordinates retain existing route
bounds; arithmetic uses checked i128 before any model conversion.

Use the existing exact mirrored/quarter-turn transform for every route-side
explicit pad center. Swap width/height for odd quarter turns. A centered extent
of integer diameter or width `d` uses conservative radius `ceil(d/2)`. Let
`r = ceil(route_width/2)`, `c = copper_clearance`, `h = hole_to_copper_clearance`
and `e = edge_clearance`, all nonnegative integer Nm.

- Foreign or unassigned route-side rectangular pad boxes expand each half extent
  by `r + c + 1 Nm`. Same-net pad copper is not a clearance obstacle.
- Foreign or unassigned route-side track endpoint bounding boxes expand by
  `ceil(track_width/2) + r + c + 1 Nm`. This conservatively encloses the capsule.
- Foreign or unassigned through-via copper boxes expand their center by
  `ceil(via_diameter/2) + r + c + 1 Nm` on each axis.
- **Every** through-via drill hole, including same-net and unassigned holes,
  contributes a box with radius `ceil(drill/2) + r + h + 1 Nm`. No existing via
  attachment exception is introduced.
- Inset the rectangular board domain by `r + e + 1 Nm` on every edge. A collapsed
  domain rejects. Exact endpoint centers and their nearest-grid anchors must fit
  this domain. Conservative boxes outside the domain are irrelevant; intersecting
  boxes may be clipped to it without weakening protection inside the domain.

The extra 1 Nm and outward half-width rounding are conservative. Closed box
contact counts as blocked. Diagonal copper boxes and endpoint-stub boxes may
reject physically clear possibilities; no completeness/optimality claim is made.
All relevant pad/track/via geometry is validated before planning. Existing
unrelated schematic content is never cloned or inspected.

## Bounded deterministic path

First snap the exact endpoint centers to their nearest grid anchors with the
existing signed ties-away-from-zero rule. Preserve the exact centers with short
endpoint stubs. A stub whose bounding box touches any obstacle rejects; the
planner does not search for alternative pad escape geometry.

Construct sorted unique X/Y coordinate sets from both anchors, inward-rounded
domain boundaries and grid coordinates strictly outside each obstacle's min/max
edge. Signed floor/ceiling division is exact; strict escape coordinates use
`floor((min - 1)/grid)*grid` and `ceil((max + 1)/grid)*grid`, clipped to the domain.
Cartesian nodes inside/touching an obstacle are blocked. Adjacent coordinates in
each row/column form candidate cardinal edges; an edge touching any obstacle is
blocked. All points stay in the inset rectangle.

Find a shortest Manhattan-length path **within this finite graph**, using a
minimum-priority queue with stable coordinate/node ordering for equal costs.
No hidden random choices or existing copper movement are allowed. Remove only
intermediate collinear points lying between their neighbors. Feed the resulting
grid waypoints to the existing exact cardinal posture planner, which preserves
the endpoint stubs and segment bounds.

Hard limits are checked before graph/result allocation:

- Existing bounded board capture: at most 100000 combined supported objects and
  existing per-string/aggregate text limits; reserve final candidate capacity.
- At most **64 relevant obstacle boxes**, including separately counted copper
  and hole boxes; at most **20000 Cartesian nodes** and **100000 candidate edges**.
- At most **4000000 aggregate node/edge/stub versus obstacle box tests**.
- At most **510 returned interior waypoints**, **1024 generated segments** and
  existing total copper capacity. No path or any limit rejects explicitly.
- At most **two retained planning/preflight operations** across preview queries
  and recomputing walkaround commands. Busy requests reject immediately. A
  cancelled UI receiver does not release a still-running worker's admission.

Return plan points, exact source project/cursor, derived net/layer and actual
bounded work counts. A valid empty-obstacle direct path still uses the same
bounded algorithm and checks. Graph shortest length is not a global PCB route
or minimum-corner guarantee.

## Shared immutable preview and atomic commit

`plan_pad_walkaround` accepts project/full event cursor guards, endpoint IDs,
grid/width and the existing DRC rule DTO. Capture only bounded board data while
holding the engine lock, then perform graph search and **complete shared joint
route DRC** on that immutable capture. Return no successful plan whose generated
segments/stubs fail that snapshot's scoped DRC. Preserve the entire live
project, selection, dirty state, event cursor and history. The result is labeled
with its captured cursor; concurrent changes can invalidate it.

`route_pad_walkaround` accepts the same guarded intent, recomputes the bounded
path from live current board geometry and runs the same complete joint DRC.
The generic transaction cursor guard rejects stale streams/sequences before
application. Existing shared candidate logic checks all generated tracks and
rejects unsafe, truncated or incomplete reports before any live mutation. One
transaction creates all tracks with new stable IDs; one undo removes the whole
route and redo restores exact geometry/IDs. Existing unrelated scoped DRC
violations may remain; this is not whole-board certification.

## Desktop interaction and agent parity

The pad-route settings offer **Static cardinal walkaround** as an explicit
choice. Pick compatible exact pads or their visible guide, then request a plan.
One background worker retains a guarded snapshot; pending controls are disabled
and the existing local draft remains cancellable. Do not run graph/DRC on hover.
Only a successful complete same-snapshot preflight becomes the highlighted local
preview. **Escape** cancels; **Enter/Finish** invokes the shared recomputing
command. A late worker result with stale project/cursor/endpoints is discarded.
Repeated activation and conflicting modal operations preserve/cancel drafts
through existing guards. Manual waypoint editing belongs to the existing tools;
this static choice has no intermediate-waypoint editor.

Every desktop planning/commit operation has the matching shared MCP query or
engine command/action schema. Errors preserve useful local inputs for retry.
No model mutation occurs for preview, cancellation, stale/busy/no-path refusal,
or invalid/unsupported/incomplete checks.

## Required evidence and limits

Independent tests must verify a direct blocked path and expected detour, both
corridors and closed/no-path domains, signed/odd width/grid rounding, exact
off-grid stubs, pad/track/via obstacles, **same-net/foreign/unassigned drill holes**,
layer/net filtering, unsupported geometry and every budget/refusal. Engine/MCP
tests require unchanged full sessions on failure, two-agent stale guards,
preview/commit same-snapshot path equality, complete joint DRC and one undo/redo.
Actual desktop input/worker tests cover picking, plan button, pending/cancel,
late result discard and commit. Retain real production MCP and readable native
evidence, actual small-board synchronous commit timings and all Linux gates.

No 45-degree walkaround, arbitrary outline/cutouts, vias in the new route,
existing track edits, shove, differential pairs, plane/bus routing, pad-edge
inference, full physical continuity/fabrication certificate, worst-case latency
or native 60 fps claim is introduced. Conservative boxes and fixed budgets may
refuse valid physical routes; manual tools remain explicit alternatives.
