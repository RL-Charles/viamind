# Static cardinal walkaround verification

## Scope and provenance

T81 adds a bounded deterministic cardinal search between exact same-side explicit
pad centers. Both the immutable plan and guarded recomputing commit require the
same complete scoped joint DRC. The [operator guide](static-cardinal-walkaround.md)
and [public-source spec](specs/static-cardinal-walkaround.md) describe the exact
integer rounding, rectangle/no-cutout scope, work limits and conservative refusals.

The production binary below was built from
`bcbeffe156adf62768a853bfb5d1fa44b340cbfd`, based on main `3c55c9f`, before
an additive fabrication-controls rebase and a one-label correction to walkaround
instructions. Its SHA-256 is
`633182d74626014d3383879d98019782074ec148560cd204cd613cf2b843a41f`.
Historical evidence identifies that source; final combined CI checks the
integrated branch separately.

## Linux gates and meaningful checks

[Gate summary](verification/t0081-linux-native.json): formatting, strict workspace
Clippy over all targets, workspace tests including doctests, and Linux workspace
build passed under the shared external mutex. **516 passed, zero failed, one
ignored**. Workspace source timestamps were refreshed inside the mutex so shared
Cargo metadata could not silently reuse a different worktree's local crate API.
Third-party cache reuse remained enabled. Artifacts/logs and `TMPDIR` were moved
under the home filesystem after an earlier `/tmp` quota interruption; the
interrupted zero-byte log was not treated as evidence.

Independent tests cover exact deterministic obstacle detours, signed odd-grid
rounding, closed contact/stub refusal, shortest finite-graph distance, narrow
corridors, no path and work budgets. Shared engine tests exercise typed top and
dynamic bottom routes, foreign/unassigned copper, ignored same-net/opposite-layer
copper, all via drill holes, quarter-turn/mirrored pad extents, malformed and
unsupported transforms/outlines, incomplete/truncated DRC, stale edits and exact
one-operation undo/redo. A large unrelated schematic remains unchanged and is
excluded from board capture.

Actual egui pointer, checkbox and keyboard tests select endpoints, request the
background checked plan, refuse unchecked Enter/manual waypoints, cancel with
Escape, commit atomically and undo/redo. A retained worker survives draft cancel
until completion; a late successful result cannot publish into a cancelled or
changed draft. Live UI/MCP action catalogs match. Existing manual, posture and
through-via route checks remain in the full gates.

## Actual production desktop HTTP

The unchanged production executable was launched through Cua Driver with MCP on
`127.0.0.1:7907`. [Window metadata](verification/t0081-production-window.json)
confirms the real desktop process; the compositor allocated a **1 × 5** tiled
window. That image is not offered as readable visual verification. Existing
previews and the user's desktop configuration were preserved.

The [retained verifier](verification/t0081-walkaround-http.py) makes real MCP HTTP
requests through two independently initialized clients. Its
[23 passing checks and measurements](verification/t0081-native-http.json) include:

- Immutable full-session planning and independent exact seven-point path with
  two off-grid stubs around a foreign copper barrier.
- Six committed segments equal the checked plan, derived net/side and bounded
  work counters; one event, one undo, exact identity-preserving redo.
- Peer stale query/commit, zero grid/width, identical endpoints, collapsed domain,
  incompatible side changes and same-project reopened stream refuse unchanged.
- A same-net via drill hole still causes a checked detour. A full-height barrier
  refuses plan and commit without mutation. Connectivity querying stays separate.

The [saved fixture](verification/t0081-route.viamind.json) contains two quarter-turn
explicit pads, a foreign track barrier and the final committed detour. The first
HTTP plan took **5.05 ms** and the first recomputing commit **5.46 ms**. Five warm
commit round trips were **6.58, 41.37, 6.44, 5.46 and 5.48 ms**; mean **13.07 ms**.
These include HTTP and synchronous complete scoped DRC on this small fixture.
Concurrent builds were active; a nearby host load snapshot was 23.61/18.24/17.49.
The outlier is retained. These measurements establish functionality and observed
small-board responsiveness, without a worst-case latency or idle-system claim.

## Readable native inspection

The [readable native capture](verification/t0081-checked-preview.png) shows the
orange checked detour around the red foreign track, exact J1/J2 attachments,
updated walkaround instructions, complete checked-snapshot status, six segments,
one obstacle and 30 graph nodes. Both Finish and Cancel controls are visible.
The guide stays separate while the route is only a local preview.

The [retained inspection host](verification/t0081-inspection.rs) runs the production
application controls and egui pointer events through the same actual UI harness,
asserts the complete engine session unchanged, and then renders that local state
with native wgpu at a fixed 1600 × 1000 viewport. It retains logical egui focus
inside this inspection host only; compositor background focus loss otherwise
correctly cancels the local draft. It sends no operating-system input and makes
no claim of OS-level pointer automation. Production focus-loss behavior is
unchanged.

[Provenance](verification/t0081-inspection-provenance.json),
[window metadata](verification/t0081-inspection-window.json) and
[runtime assertions/points](verification/t0081-inspection-runtime.log) identify
source `fd9aab1649454087ce8980354d99f312303c156d`, the unique inspection binary,
fixture preparation and actual 1568 × 980 capture pixels. The inspection uses
production default grid/width **0.25 mm**, so its upper escape row is **7.5 mm**
and return column **20.5 mm**. The separate production HTTP fixture uses a
**1 mm** grid and **0.250001 mm** width, producing escape row **7 mm** and return
column **21 mm**. Both preserve exact off-grid pad centers and use the complete
shared scoped preflight. Every native inspection frame asserts an unchanged full
engine session and nonempty checked points.

The integrated source's Linux formatting, Clippy and build/test CI all passed:
[run 38131282776](https://github.com/RL-Charles/viamind/actions/runs/38131282776).
Artifact-only changes require one final CI run on the submitted PR head.

## Limits

Only axis-aligned rectangular outlines without cutouts and same-side exact
quarter-turn explicit endpoints are supported. Copper is static. Conservative
boxes can refuse physically clear paths, especially diagonal tracks and endpoint
stubs. Search minimizes Manhattan distance in its finite graph, without a minimum
corner or globally shortest physical-route claim. Exceeded capture, obstacle,
graph, intersection, result or retained-operation budgets refuse explicitly.

Complete scoped DRC is required; existing unrelated violations can remain and
published checker exclusions still apply. This is no physical connectivity,
manufacturing or design-rule certificate. Existing conservative stored-endpoint
proofs control connection guides separately. No shove, automatic vias, arbitrary
outline walkaround, differential-pair or plane routing is added. The strict native
100k-object **60 fps target remains open**.
