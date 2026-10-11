# Route through stable existing copper terminal identities

Ticket T-0106 · Parent S-059. Public-source contract before implementation.

## Public behavior and supported subset

[KiCad 9 PCB routing documentation](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#routing-tracks)
describes starting routes on existing copper, object snapping at pads/vias,
temporary route geometry, explicit posture switching, and optional track-width
inheritance when starting at an existing track end. Reviewed 2026-10-11 for
behavior only. Viamind implements its own exact identity resolution and UI.
No implementation or assets are copied.

The current Viamind identity picker accepts explicit pads only. Add a useful
workflow for continuing a partial route or joining existing copper without
retyping its endpoint coordinates: choose a persisted explicit pad, track start
or end, or existing through-via center as either terminal. Route on one explicitly
chosen outer copper layer. Width remains an explicit input; existing objects are
never resized or moved. This increment adds no midsegment contact/split, new via,
inner layer, inferred size, obstacle avoidance, shove, physical continuity
certification or automatic routing-completion claim.

Existing pad/polyline/posture/walkaround/routed-via tools and schemas remain
unchanged. New terminal tools are additive, with no native or event version
change: successful commands create ordinary existing Track deltas.

## Strict identities and bounded discovery

Use a strict tagged terminal identity with exactly these variants:

- `pad`: persisted `part_id` and `pad_id`.
- `track_end`: persisted `track_id` and `end` (`start` or `end`).
- `via`: persisted `via_id`; the command's explicit layer selects its face.

Unknown tags/fields, missing IDs and invalid endpoint names reject. IDs resolve
against the live board; command inputs never supply the terminal's coordinate,
net, width or inferred owner metadata. Track endpoint coordinates are its exact
stored integer Nm. Via center coordinates are its exact stored integer Nm.
Pads reuse the existing exact quarter-turn parent transform; unsupported
arbitrary-angle pads remain unsupported. Requested layer must equal a pad or
track's actual outer layer; a valid existing through via supports either outer
layer. Both terminals require the same nonempty exact case-sensitive assigned
net, at most256 UTF-8 bytes, and distinct identities and positions. Unassigned,
missing, invalid basic geometry, incompatible-layer or changed-net terminals
reject atomically. Existing endpoint geometry is not repaired.

Add an immutable `query_route_terminals` tool and corresponding desktop capture.
Each row identifies its terminal and layer, exact center when supported, bounded
human-readable label/net and an unsupported reason. A via has one row per outer
layer; `(terminal identity, layer)` distinguishes rows. Optional exact net/layer
filters, offset and limit1..1000 (default100), total and next offset support
deterministic pagination. Sort by terminal tag, UUID identity, endpoint and layer.
Include project UUID/full source cursor; restart pages after any source change.
Offset arithmetic saturates, including `usize::MAX`.

Before copying labels, allocating rows or geometry-only snapshots, inspect
borrowed board structure/text and document-policy bounds with the existing route
budget (100000 structural objects, 2MiB geometry text and512-byte labels).
Additionally cap total terminal rows at100000, counting all pads, both track
ends and both via faces before filtering. Use checked counts before allocation;
over-budget input refuses rather than returning a deceptively complete partial
picker. Exclude unrelated schematic content and unused assembly metadata from
copies. Preserve bounded named-net policy capture. Do not clone the project or
recompute electrical guides for terminal discovery. No all-pairs endpoint work.

## Shared exact path commands and atomic validation

Add `route_terminal_polyline` and `route_terminal_posture`, corresponding to
the existing manual-pad and cardinal/octilinear-pad route operations. Both take
expected project UUID/full initial cursor, two strict terminal IDs, explicit
outer layer, positive integer Nm grid/width and optional existing DRC input.
Polyline interior vertices are supplied already grid-aligned (max1023).
Posture waypoints are supplied already grid-aligned (max510), with existing
cardinal/octilinear mode and explicit primary/alternate posture semantics.
Reject unbounded raw arrays before secondary typed allocations over MCP.

Resolve terminals afresh, derive one exact net, then reuse existing shared path
math and copper preflight. Manual endpoints retain their exact off-grid stored
coordinates, while interior vertices are grid-aligned. Posture paths retain
exact off-grid terminals and existing short nearest-grid stubs; stubs can have
arbitrary angles and participate in all scoped checks. Preserve exact negative
snapping/tie/range behavior; at most1024 generated segments, existing copper
coordinate/width/grid limits and candidate/check budgets apply.

No model mutation occurs before complete shared scoped joint DRC on every
proposed segment against one bounded board candidate. Request baselines cannot
weaken captured document minima. Reject violation, incomplete/truncated report,
stale stream/sequence/project or changed/missing terminal with no partial tracks,
history append, selection change or dirty-status change. A successful command
commits all segments in one event/undo; redo restores their exact IDs. Typed,
dynamic and batch execution preserve the existing initial-transaction cursor
contract. Stored terminal objects and existing guide-proof behavior are intact.

Shared DRC retains its documented explicit-geometry coverage/exclusions and
preexisting-violation policy; adding a segment does not certify all existing
copper, remove duplicate tracks, or prove that a track reaches a via's annulus.
Guide suppression remains the separate conservative stored-endpoint proof.

## Desktop interaction

Provide a clearly labeled terminal-routing entry point with explicit layer,
width/grid and manual/cardinal/octilinear controls. Capture terminal identity
rows once at draft start; use bounded screen-space endpoint hit targets and
deterministic identity tie-breaking for coincident choices. An existing via on
both faces resolves through the chosen layer. Show the selected terminal kind,
identity/label and exact center/net/layer; do not silently choose a midpoint or
round an attachment to the grid. Unsupported terminals disclose the reason.

Start/target selection and optional interior/waypoint clicks update only local
draft geometry. Preserve exact attachments in preview, snapped pointer vertices,
explicit posture toggle and Backspace removal. Local geometry is explicitly
unchecked until guarded commit; no collision-free preview promise is made.
Finish/Enter calls the shared terminal command. Escape/Cancel discards the draft
without model changes. Current-source mismatch cancels or invalidates the draft
before any commit, including same-UUID reopen. Existing route/editor/modal and
schematic-mode guards remain effective. Persistent Finish/Cancel controls fit a
640x400 viewport; scrolling cannot hide the only commit/cancel path.

## Verification and honest limits

- Independent public-source spec approval precedes Rust edits.
- Independently expected exact geometry for pad/track/via combinations, both
  layers, negative/off-grid coordinates, both postures and snapping ties.
- Net/layer/missing/unsupported/coincident/malformed identity refusals leave the
  complete session unchanged; mutations between preview and commit, undo/redo,
  same-UUID reopen and typed/dynamic/batch initial-cursor boundaries are covered.
- Explicit width below document minima refuses; equality passes without sizing
  inference. Foreign-copper/hole/outline and incomplete-check failures reject all
  proposed segments, including off-grid stubs. Existing object bytes stay exact.
- Discovery boundary/count/text/policy tests charge before owned allocations;
  via-face pagination, exact filters, offset overflow and unused huge assembly or
  unrelated schematic exclusion are covered. Existing route APIs remain passing.
- Actual egui pointer/keyboard tests select track ends and via centers, create
  local preview, change posture, cancel and guarded-finish; compact footer bounds
  and modal/stale guards are checked independently from native transport proof.
- Actual initialized two-client production HTTP tests exercise new discovery and
  both commands, peer visibility, atomic refusal/one undo and no query mutation.
- Linux fmt, strict workspace all-target Clippy, workspace tests/build, exact
  full-app generated tool-reference/workflow gates and final combined CI pass.
- Durable source/binary/runtime provenance and readable self-closing native WGPU
  wide/compact captures verify exact selected terminal markers, preview, controls
  and full-session invariants. Keep synthetic input, native framebuffer output,
  actual production transport and OS input claims distinct. No cadence claim;
  the existing strict100000-object60fps acceptance remains open.
