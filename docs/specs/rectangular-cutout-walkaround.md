# Checked static walkaround around rectangular substrate cutouts

## Public sources and independent design

Read before implementation on 2026-10-11:

- [Public PCB Editor manual, board outlines and routing](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html): closed interior outlines remove substrate; walkaround keeps existing objects stationary. Its board-edge rules apply to internal cutout boundaries as well as the outer perimeter.
- [Rust integer division](https://doc.rust-lang.org/std/primitive.i128.html#method.div_euclid): signed integer floor/ceiling arithmetic for exact outward grid escape coordinates.
- Existing independent [static cardinal walkaround spec](static-cardinal-walkaround.md) defines Viamind's finite graph, exact endpoint stubs, admission/work limits and complete shared preflight.

No external implementation code, geometry data, assets or algorithm is copied.
The narrow rectangle geometry, margins, refusal semantics and tests below are
independent Viamind choices. General polygon walkaround and shove remain open.

## Behavior

The existing `plan_pad_walkaround` and `route_pad_walkaround` input/schema and
same-side explicit endpoint workflow accept a valid axis-aligned four-corner
rectangular outer outline with zero or more valid **axis-aligned four-corner
rectangular cutouts**. Every ring must pass existing whole-outline validation:
strictly internal disjoint cutouts, bounded coordinates and ring/vertex budgets.
Reject nonrectangular, diagonal, degenerate, touching, overlapping or malformed
cutouts explicitly before search. A four-point bounding box alone does not make
a ring rectangular; each vertex must be an exact bounding-box corner and each
edge axis aligned. Winding/start corner do not change accepted substrate geometry.

Each cutout is excluded for every route net and copper side. Define:

```
r = ceil(route_width_nm / 2)
cutout_margin = r + edge_clearance_nm + 1 Nm
cutout_obstacle = exact cutout bounds expanded by cutout_margin on all sides
```

Use checked i128 arithmetic before converting to stored/rendered Nm. This is
**board-edge clearance**, independently of copper clearance and via-hole-to-copper
clearance. Closed node/edge/stub contact with the expanded box is forbidden;
existing signed integer escape rounding chooses the next grid coordinate
strictly outside each closed box. Odd widths round outward. Exact off-grid pad
centers and their short attachment stubs remain unchanged and receive the same
closed obstacle exclusion and complete shared DRC as the generated route.

The outer substrate domain stays inset by the same route half-width + board-edge
clearance + strict one-nanometre margin. Clip cutout exclusions to that domain;
expanded cutouts may block a corridor or the whole domain. Refuse endpoints/stubs
inside exclusions and explicit no-path cases without a successful preview or any
mutation. Existing copper/vias/parts stay stationary; no clearance relaxation or
fallback route mode is allowed.

## Budgets, checked preview and atomic commit

Cutout boxes and existing foreign/unassigned copper/all via-hole boxes share the
existing **64 relevant obstacle boxes total**, 20000 nodes, 100000 candidate edges,
4000000 aggregate box intersection tests, 510 returned interior waypoints and
1024 generated segment budgets. Do not introduce another independent cutout-box
allowance. The existing bounded board-only capture and two retained operations
per process stay unchanged. `work.obstacle_boxes` counts the combined relevant
substrate/copper/hole boxes.

The immutable query uses a captured full source cursor and runs outside the
engine lock after bounded capture. Every successful query must pass the existing
complete scoped joint DRC, including all cutout boundaries and endpoint stubs.
The guarded command recomputes the path and the same complete joint DRC against
the live initial transaction cursor, then applies all new segments atomically.
Incomplete/truncated reports, stale edits/reopens, malformed scenes, unsupported
cutouts, no path or exceeded budgets leave the complete design session unchanged.
One undo removes all route segments; redo preserves their exact identities.

The desktop uses the existing explicit static mode, retained background worker,
checked-only local preview, endpoint/guide picking, cancellation and Finish/Enter
workflow. Update current descriptions/labels to say rectangular outer/cutout
scope; preserve historical T81 verification reports and captures unchanged.
Every desktop action remains the same shared MCP query/command.

## Required independent verification

- Derive exact expected paths around one and multiple cutouts; exercise both
  windings/start corners, top/bottom, mirrored/quarter-turn off-grid endpoints,
  odd width, signed coordinates and exact strict-margin/grid boundaries.
- Demonstrate changing edge clearance changes the cutout path while copper
  clearance alone does not. Test endpoints/stubs inside or touching cutout
  exclusion, blocked corridors, no path and malformed/nonrectangular cutouts.
- Count cutouts together with copper and all drill holes at the shared budget
  boundary; reject overflow unchanged. Retain existing no-cutout route behavior.
- Immutable plan/full-session equality; typed/dynamic/MCP recomputed commit equals
  checked geometry, one event and undo/redo. Real peer outline edit and same-project
  reopen must invalidate old query/commit and desktop checked preview.
- Actual desktop control/pointer/key test requests checked cutout preview, cancels
  without mutation and commits once. Preserve old manual/posture/via tests.
- Actual production two-client HTTP checks with retained exact expected geometry,
  observed small-fixture planning/commit latency and readable native inspection
  provenance. Distinguish inspection logical input from OS automation.
- Linux formatting, strict workspace all-target Clippy, workspace tests/build and
  final combined CI before merge; all gates serialize through the shared mutex.

No arbitrary polygon walkaround, 45-degree avoidance, shove, moving obstacles,
physical/manufacturing certification, worst-case latency or native60fps claim.
