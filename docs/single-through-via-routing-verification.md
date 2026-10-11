# Single through-via routing verification

## Reviewed scope

T-0073 implements one guarded plated through-via transition between opposite-side,
same-net explicit quarter-turn SMD pads. The public-source contract is
[the behavior spec](specs/single-through-via-routing.md). Desktop and MCP use
`route_pad_via_polyline`; ordinary and same-side pad routes share the same bounded
joint candidate preflight.

## Linux gates and input checks

Code head `b58983fdf2ad459fb01712234053a1db2484ed92`, based on merged cross-probing
main `4618c28`, passed:

- `cargo fmt --all -- --check`
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`
- `cargo test --offline --locked --workspace`: **433 passed, zero failed, one existing ignored**,
  including doctests.
- `cargo build --offline --locked --workspace`

The whole sequence used `/tmp/viamind-workspace-gates.lock`, a shared dependency
cache and a workspace Rust mtime preflight. No source edits occurred during it.
Before integration, 34 focused engine/UI/MCP tests passed, including ordinary
polyline and same-side pad routing regressions after the helper refactor.

Engine checks resolve exact top/bottom 90-degree centers, derive net/layers,
verify ordered track/via IDs and one undo/redo, and reject invalid dimensions,
zero lengths, off-grid interiors/via, missing/unsupported/incompatible pads,
stale grouped proposals, foreign copper on both sides, nearby same-net via hole
overlap and incomplete reports without changing session/model/history/dirty state.
Actual egui pointer/key tests exercise pad/guide picking and hidden layers,
local snapped via/start-leg/target-leg edits, V replacement preserving interiors,
Backspace per stage, endpoint replacement clearing geometry, Escape/focus cancel,
Enter commit and concurrent side-edit rejection. Existing free/same-side route
input tests also pass. Two independent MCP contexts verify the same command,
exact attachment, stale rejection and one undo/redo.

## Actual production native HTTP checks

A saved copy of the verified production application ran as PID `201388`, MCP
port `7898`. Its SHA256 is
`e92e1a1030a74b613cdfe8b3342dffc713898d5a8c6965c1efe3dab011d4728d`.
[Twenty two-agent HTTP assertions](verification/t0073-native-http.json) passed,
including native tool discovery, exact off-grid transforms/attachment, derived
layers/net, joint tracks/via commit, malformed/stale rejection, physical hole
spacing, concurrent side edits, undo/redo identities and persistent conservative
guides. The [standalone check script](verification/t0073-native-http.py) requires
a fresh owned native application on its supplied port and its binary path.

The first four-segment/via commit measured **9.90 ms** wall time over HTTP; five
warm commits measured 2.39, 1.75, 1.71, 2.44 and 2.15 ms, mean **2.09 ms**. These
are small five-part debug-board observations on a shared, loaded machine,
including HTTP overhead. They establish neither worst-case bounded-board latency
nor native frame cadence. DRC is synchronous under the engine lock at commit;
local preview does not run DRC. The strict native 60 fps target remains open.

The actual production window was captured successfully at its compositor-assigned
105×131 size; it was too small for useful inspection. No resize/input fallback
was attempted. Existing previews were preserved.

## Readable native visual inspection

A temporary fixed 1600×1000 viewport wrapper used the **production ViamindApp**,
engine, renderer and [saved native fixture](verification/t0073-route.viamind.json).
Its [complete wrapper source](verification/t0073-native-inspection.rs) was compiled
under the shared mutex, copied, and removed from the Cargo examples directory.
Wrapper binary SHA256:
`b0ce887c1fbe03d607e9d0a80b5c730208132480c3469b9b45ec8b33519f4612`.
This is inspection evidence, distinct from the production HTTP executable and
actual event-driven input tests. No native click/type success is claimed.

- Board wrapper PID `203450`, window `110379503966832`:
  [native board image](verification/t0073-routing-board-native.png).
- Settings wrapper PID `204175`, window `110379504458016`:
  [native settings image](verification/t0073-routing-settings-native.png).

Fresh Cua captures were 1568×980. Both were inspected: two pairs of red top/blue
bottom legs meet through-via rings; exact explicit pads, outline and both unchanged
connection guides are visible. Settings show width, grid, copper/edge clearance,
via diameter and drill, plus scoped preflight/continuity limits without clipping.

## Remaining limits

Only one new plated through via between opposite outer-side rectangular SMD pads
is supported. Arbitrary-angle attachment refuses; exact quarter-turn centers may
be off-grid. Via and supplied interiors must be grid aligned. No inner layers,
blind/buried/microvias, multiple transitions, track-endpoint attachment, automatic
obstacle avoidance, walkaround or shove is implemented. Preflight inherits scoped
DRC exclusions, including legacy graphical pads and unsupported footprint/hole
geometry; existing unrelated violations may remain. Bounded capture counts all
proposed copper, excludes unrelated schematic content and rejects incomplete
checks. Copper continuity is not verified, and conservative pad-only guide counts
remain after routing. This increment does not complete the routing story.
