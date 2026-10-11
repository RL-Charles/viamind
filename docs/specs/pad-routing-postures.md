# Exact cardinal and 45-degree pad routing postures

## Public behavior references

The official [KiCad 9 PCB editor manual, Track posture and Track corner mode](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#track-posture)
explains choosing the order of straight and diagonal segments, and cardinal-only
versus horizontal/vertical/45-degree paths. This informs independent behavior;
no implementation code or assets are used. Viamind chooses the posture explicitly
and performs no automatic obstacle handling or inferred pad-edge orientation.

The [egui Response API](https://docs.rs/egui/latest/egui/response/struct.Response.html)
provides canvas click/hover events for the local draft. Existing integer-Nm,
exact endpoint, guarded transaction and scoped DRC contracts remain authoritative.

## Integer geometry contract

A pure router planner accepts exact start/target points, at most 510 ordered
waypoints, positive origin-anchored grid, corner mode and posture. Coordinates
remain integer nanometres. Grid snapping uses existing nearest/ties-away-from-zero
semantics and checked i128 intermediates. The supported model coordinate range
is ±10000 mm; grid is positive and at most 10000 mm.

Modes and explicit postures are:

- `cardinal`: horizontal-first or vertical-first.
- `octilinear`: straight-first or diagonal-first; every constrained segment is
  horizontal, vertical or has exactly equal absolute x/y deltas.

At each pair of snapped anchors, cardinal planning uses the chosen axis bend.
Octilinear planning uses one diagonal of length min(abs(dx),abs(dy)) plus the
remaining major-axis straight length in the chosen order. Single-segment cases
need no bend. Every bend remains on grid because snapped anchor deltas are grid
multiples. Posture is one explicit value applied consistently to all anchor pairs;
no mouse-history or pad-orientation inference occurs.

Both pad endpoint centers are independently resolved exactly from persisted IDs.
The planner connects each center to its nearest grid point using an explicit
short stub when needed, then plans the constrained path between snapped anchors.
These two endpoint stubs may have arbitrary angles. Each stub's absolute x/y
extent is at most grid/2 rounded upward; they are not claimed to satisfy cardinal
or 45-degree constraints. All stubs are included in the same scoped DRC candidate.
The command never rounds the actual pad attachment or converts presentation floats.

Supplied waypoints must already be grid aligned; desktop clicks alone snap.
Consecutive duplicate generated points are removed, preserving exact endpoints
and ordered waypoint visits. Nonconsecutive revisits remain explicit; there is no
loop elimination or acute-angle optimization. Result size is at most 1025 points
(1024 segments). Invalid/out-of-range input, impossible nonzero route or exceeded
output capacity rejects with an actionable error and no partial plan/commit.
The returned plan identifies endpoint stub count and generated segment count.

## Shared guarded command and desktop

`route_pad_posture` accepts expected project/full initial cursor, start/target
`PadEndpointId`, grid, width, grid-aligned `waypoints` (default empty), mode,
posture, and optional scoped DRC rules. Same-side, distinct, exact same assigned
net explicit rectangular quarter-turn SMD pads are required. Net and copper side
are derived. Opposite-side routing continues through the existing explicit via
command; this increment adds no transition behavior.

The engine calls the same pure planner as the desktop, then shares bounded joint
preflight with ordinary/same-side/via routes. Whole proposed geometry, including
endpoint stubs, is checked before any mutation. Existing cursor, board-object/text,
net, outline and complete-report requirements apply. Stale/malformed/unsafe or
incomplete proposals reject all copper. One undo/redo preserves every new ID.
The old `route_polyline`, `route_pad_polyline` and `route_pad_via_polyline` tools
retain their manual behavior and public signatures.

Desktop pad routing adds an explicit corner-mode choice in settings. Manual is
the existing default. Cardinal or 45-degree mode adds a posture toggle in the
local preview panel. Clicking empty canvas fixes snapped waypoints; generated
bends are previewed separately from clicked targets. Changing posture recalculates
local geometry and creates no model/history/selection events. Backspace removes
one confirmed waypoint; Escape cancels; Enter/Finish uses the shared command.
Endpoint picking, guide visibility, exact IDs, stale notices and modal guards
remain shared. The preview visibly identifies generated endpoint stubs and remains
unchecked until commit. Via/manual routing continues unchanged.

## Limits and verification

No obstacle avoidance, automatic walkaround, push/shove, pad-edge escape planner,
rounded corners, length tuning, differential pairs or track-endpoint attachment.
No copper continuity certification, full DRC or native 60 fps claim. Guide behavior
follows the independent connectivity implementation; this planner does not alter
that API or its proof coverage.

Tests cover signed quadrants, equal-axis/diagonal cases, explicit postures,
odd grids and exact off-grid stubs, duplicate/revisited targets, overflow/range,
input/output budgets and immutable failures. Engine/MCP checks cover shared path,
exact guarded identity/net/layer, scoped preflight, stale/batch rejection and one
undo. Actual pointer/key checks change posture, snap waypoints, backspace/cancel,
commit and undo; existing manual/free/via suites remain regression gates. Native
HTTP/readable visual evidence and all Linux fmt/clippy/test/build gates precede
submission.

Initial stacked base is T73 final `115b423a31d55c070e1d4d876f50789bf9b711c7`.
Transplant only T77 commits onto merged main before final CI/PR.
