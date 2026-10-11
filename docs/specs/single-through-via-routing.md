# Guarded single through-via pad routing

## Public behavioral sources

- [KiCad 8 PCB editor manual](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html), “Placing vias,” describes a through via as a transition between routing layers, a local placement that can be canceled, and explicit via size selection.
- The same manual's “Editing track and via properties” describes independent copper diameter and hole diameter.
- [egui 0.36.2 Response](https://docs.rs/egui/0.36.2/egui/response/struct.Response.html) documents pointer and keyboard interaction responses used for the local draft.

These public behaviors motivate this independent scope; no upstream code/assets
are copied. Existing [exact pad routing](explicit-pad-routing.md), the persisted
through-via model and scoped DRC provide the shared foundation.

## Shared command

`route_pad_via_polyline` accepts exact `expected_project` / initial
`expected_cursor`, persisted `start` / `target` part-pad references, integer-nm
`via` position, `before` and `after` interior point arrays, positive integer
`grid` / `width` / `via_diameter` / `via_drill`, and optional scoped `rules`.
Both arrays default to empty. Their combined length is at most 1022, giving at
most 1024 tracks and one via in a single transaction.

The command resolves pad identities from the guarded live candidate using the
same checked integer quarter-turn transform. It requires distinct explicit SMD
pads with the exact same assigned net, representable centers, and opposite
supported outer copper sides. Net and both track layers are derived; callers
cannot override them. First/last pad centers remain exact even off-grid. The via
and all supplied interior points must already be grid aligned. The desktop alone
snaps pointer positions. Zero-length adjacent segments, unsupported pad angles,
invalid/missing pads, mismatched nets/sides, invalid dimensions, coordinate
limits and stale session/project reject the entire proposal.

The first leg is `start + before + via` on the start pad's side. The second is
`via + after + target` on the target pad's side. One circular plated through via
at the shared position spans both outer sides, with the same exact net label.
Drill must be positive and strictly smaller than copper diameter. Existing
minimum track width, via drill and radial annular width rules apply.

Before cloning any board data, enforce the existing 100000 structural-object
capture budget including proposed tracks/via, bounded board text/labels and input
net size. Unrelated project text/schematic content is not copied. Build a bounded
board-only candidate and check all proposed tracks and the proposed via together.
Any scoped issue involving a new object, incomplete/truncated report or checker
error rejects all deltas. Existing unrelated violations may remain. Copper/edge,
via-hole-to-foreign-copper and physical hole-spacing coverage is inherited from
scoped DRC; its omissions are explicit. Only a complete accepted candidate is
applied, in one history/event transaction. Undo/redo restores exact IDs, geometry,
net labels and both legs together. Typed, dynamic and grouped execution share the
same initial cursor semantics as existing guarded routing commands.

## Desktop interaction

Board → Route pads through via opens width, grid, via diameter/drill and scoped
clearance settings. Pad/guide discovery and six-screen-point deterministic
visible picking reuse exact endpoint routing. This mode accepts opposite-side
same-net pad pairs; same-side routes use the existing pad connection action.
Unsupported/mismatched endpoints provide actionable reasons. Legacy preview pads
and hidden copper layers do not intercept clicks.

After selecting the pad pair, the draft enters Place via. A grid-snapped
empty-space click places its local via and opens Edit start leg. The preview
connects exact start/target centers through the via, with separate labeled start
and target leg editing modes. Empty-space clicks append snapped vertices before
the via on the start leg, or before the target on the target leg. Buttons select
which leg to edit, replace the via, or explicitly change endpoint identities.
Changing endpoints clears the via and both leg interiors. Replacing the via
preserves leg interiors for local adjustment; invalid geometry still rejects at
commit. The V key selects Place via. Backspace removes the last interior vertex
on the active leg, or removes the local via while placing it. Escape/Cancel drops
the whole local draft. Enter/Finish calls the shared guarded command only when
both endpoints and the via are present. A rejection retains the draft/error.
Stale edits require cancel/restart. Existing modal/focus/document/workspace guards
apply; local preview never mutates shared selection or the document.

Orange copper/via previews are unchecked until commit. Display derived net and
both outer sides, active leg, exact endpoint coordinates, snapped via position,
via copper/drill dimensions, and stale-session warnings. No live DRC or automatic
obstacle avoidance is implied.

## Limits and verification

This increment supports exactly one new plated through via between opposite
outer-side rectangular quarter-turn SMD pads. No inner layers, blind/buried or
microvias, multiple transitions, existing track/via endpoint attachment,
automatic placement, posture constraints, walkaround, shove, or continuity
verification is added. Conservative pad-only guide counts remain after routing.
Parent full routing and strict native 60 fps acceptance remain open.

Verify signed exact/off-grid endpoint transforms, snapped via/interiors, derived
net/opposite layers, malformed dimensions and radial annular/drill rules,
foreign copper/outline/cutouts/physical holes, incomplete results, stale stream /
sequence / project / pad changes, whole-group rollback and single undo/redo.
Exercise actual pointer/key pad/guide picking, local via and leg stages,
Backspace/V/Escape/Enter, endpoint changes, hidden layers and modal guards.
Verify immutable MCP discovery/shared command parity, real production desktop
HTTP commit latency and separately labeled readable native images. Run Linux
fmt, strict all-target clippy, workspace tests/doctests and workspace build.
