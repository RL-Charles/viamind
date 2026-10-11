# Checked static walkaround through one fixed via (T88)

## Public sources and independent scope

The public [KiCad 9 PCB editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#routing-tracks)
describes temporary route previews, explicit through-via placement during a layer
change, and a walkaround mode that leaves existing copper stationary. Its
[placing vias](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#placing-vias)
section describes continuing on the other copper side after placing a via.
Only these behaviors inform this independent increment. No source, geometry,
assets, algorithm or UI layout is copied.

Viamind already has exact pad attachment, atomic single-via manual routing, and
bounded same-side cardinal walkaround with rectangular substrate cutouts. T88
combines these capabilities at ONE user-chosen fixed via center. It plans both
outer-side legs around static obstacles and jointly checks the complete candidate.
It does not search for a via location or move any existing object. Existing T73
manual via, posture and same-side T85 APIs retain their behavior and schemas.

## Shared input and exact geometry

Add typed `RoutePadViaWalkaround` and matching `route_pad_via_walkaround` command,
plus immutable `plan_pad_via_walkaround`. Input contains expected project UUID,
full initial event cursor, stable explicit start/target pad IDs, one integer-Nm
via center, grid, track width, via diameter/drill and the existing DRC rules.
Unknown fields reject. Distinct pads must have the exact same assigned net and
opposite outer copper sides; derive net and both layers from stored endpoints.
The existing normalized quarter-turn/mirror transform restrictions apply.

The fixed via must lie exactly on the requested positive grid, within existing
coordinate limits, and differ from both exact endpoint centers. Diameter/drill
must satisfy existing via geometry/rules. Never silently snap, relocate, resize
or replace a supplied via. Desktop picking alone uses the shared integer grid
snap before building the same typed request. Exact off-grid pad endpoints retain
their existing direct stubs to their nearest grid anchors; the grid-aligned via
requires no off-grid stub. A zero-length leg refuses.

The first leg runs from the exact start center to the via on the start layer;
the second runs from the via to the exact target on the opposite layer. Reuse
the deterministic cardinal finite-graph planner and tie order. Only valid
axis-aligned four-corner outer rectangles and rectangular cutouts are supported.
Every cutout is excluded by `ceil(track_width/2) + edge_clearance + 1 Nm` on
both sides. Relevant foreign/unassigned pad/track/via copper uses copper
clearance; ALL existing via drill holes, including same-net holes, use the
existing hole-to-copper rule. Preserve all existing box math, closed contact,
unsupported-transform and route-side legacy-pad coverage refusals.

The new via is not treated as a foreign obstacle to its own incident legs.
Its copper, annular, drill, edge and physical hole spacing are checked by the
complete joint candidate DRC. No planner result can be published as checked
before that candidate, including both legs and the via, passes.

## One bounded operation

Acquire ONE retained walkaround admission for the whole request, shared with
same-side walkaround's two-operation global limit. Capture one bounded board-only
snapshot and one full source cursor under the engine lock; query search and DRC
run outside the lock. No unrelated schematic clone. Commit runs its guarded
recomputation against the current transaction snapshot and mutates only after
the full candidate succeeds. Preserve the generic initial transaction cursor
guard across typed, dynamic and grouped command execution.

Both legs charge one aggregate budget: at most **64 relevant obstacle boxes,
20000 graph nodes, 100000 candidate edges, 4000000 intersection tests,
510 interior waypoints, and 1024 new track segments plus one new via**. A substrate
or existing drill exclusion used on both legs counts twice because both searches
consume it. Candidate edges/nodes must be charged before their graph allocations;
each intersection must charge the shared counter before testing. Refuse before
exceeding a cap, rather than running two full independent budgets then checking
totals afterward. Reserve capacity for all candidate objects before cloning.
The settled-node total cannot exceed charged graph nodes. Existing checker
capture, result and pair limits remain authoritative; incomplete DRC refuses.

Return project/full source cursor, derived net/layers, fixed via geometry, both
exact paths, per-leg and aggregate work counts, and complete-scoped-preflight
status only on joint success. One-leg success never escapes as a checked result.
The query leaves the complete session, selection, history and dirty state
unchanged. Commit recomputes both paths and creates all tracks plus exactly one
fresh via in one event; one undo removes the whole candidate and redo restores
the same IDs. Any stale edit/reopen, unsupported scene, no-path, budget or joint
DRC failure leaves the complete project/session unchanged.

## Desktop and agent parity

Add an explicit through-via walkaround action using existing route width, grid,
via diameter/drill and rule settings. Pick start/target explicit pads on opposite
sides and one fixed snapped via position. Request the checked plan explicitly;
manual interior vertices are unavailable in this mode. Show both checked legs,
their side identities, the proposed via, aggregate work and complete scoped
status. Unchecked straight lines must not look like a validated route.

Use the existing single retained routing worker. Poll it even while the draft
is hidden/cancelled; it retains its admission until completion. Draft token,
complete request and full cursor must match before publishing. Endpoint, via,
grid, width, via size, rule or source changes clear the checked result; stale
completion cannot revive it. Escape cancels locally, replacing the via returns
to picking and clears the plan, and Enter/Finish require a current checked
result before dispatching the recomputing shared command. Preserve file/modal,
schematic-mode and focus-loss guards. The corresponding typed action and MCP
tool are the same operation; no direct document mutation is allowed.

## Evidence and limits

Before implementation, root reviews this spec. Tests independently expect both
layer paths around different-side obstacles/cutouts, reversed endpoints,
signed/odd-grid exact stubs, and derived net/layer/via geometry. Cover aggregate
budget boundaries that each separate leg would pass, pre-allocation refusal,
joint-only via edge/annular/drill/same-net-hole failures, stale peer/reopen guards,
query immutability, one-event undo/redo, and existing manual/posture stability.
Actual desktop control/pointer/key tests cover fixed via picking, checked two-leg
preview, replacement/cancel, stale/late worker invalidation, atomic finish and
undo. Retain two-client production HTTP checks with all latency samples, readable
native checked preview and exact source/binary/fixture provenance. Linux fmt,
strict all-target Clippy, workspace tests/build and final combined CI are required.

No automatic via selection, multiple transitions, inner layers, arbitrary-angle
attachment/outline support, 45-degree walkaround, shove, copper deletion,
differential/plane routing or physical/manufacturing certification is added.
Conservative boxes can refuse clear geometry and fixed via positions can have
no route. Existing unrelated violations/checker exclusions remain explicit;
stored-endpoint guide proofs are separate. The strict native100k60fps criterion
remains open.

Initial stack boundary is T85 artifact
`1b79aedc55c4fd157a2182c37c6975c5c653a67d`; transplant only T88 commits onto
merged main before final integration. Root reviews/merges; no self-merge/done.
