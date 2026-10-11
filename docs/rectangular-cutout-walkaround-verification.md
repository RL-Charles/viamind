# Rectangular substrate cutout walkaround verification

## Scope and artifact identity

T85 extends the existing bounded cardinal walkaround to valid axis-aligned
rectangular substrate cutouts. Every cutout uses
`ceil(width/2) + edge clearance + 1 Nm`, independently of copper clearance.
Cutouts, copper and all via drill holes share the existing obstacle/work budgets.
Both the immutable plan and guarded recomputing commit require complete scoped
joint DRC. See the [public-source spec](specs/rectangular-cutout-walkaround.md)
and [current operator guide](static-cardinal-walkaround.md).

Production and inspection artifacts identify source
`fa77e1713779ae5c1e5336124bd01addcc389d0b`, based on merged T81 `bf8d455`.
The unique production executable SHA-256 is
`9143784407bfe7647042af551fa44d960ce8175318694040e38e48859e780849`.
Subsequent inherited test synchronization and unrelated main integration changes
are covered separately by focused checks and final combined CI. Historical
binary evidence remains tied to its actual source.

## Linux and interaction checks

[Linux gate summary](verification/t0085-linux-native.json): formatting, strict
workspace Clippy over all targets, workspace tests including doctests, and Linux
workspace build passed: **525 passed, zero failed, one ignored**. The complete
sequence held the shared external mutex; workspace Rust timestamps were refreshed
inside it, and unique production/inspection binaries were copied before unlock.

Three new engine tests independently expect exact top/bottom/winding, signed
mirrored off-grid, multiple-cutout and strict odd-width boundary paths. They
separate edge from copper clearance, exercise 63 cutouts plus one foreign copper
box at the combined 64-box boundary, and add a same-net drill hole to verify
65-box refusal. Nonrectangular/malformed cutouts, covered endpoints/stubs and
blocked edge corridors refuse unchanged. Actual peer-outline edits and a
same-project reopened stream reject stale query and commit. Atomic commits use
one event and one undo, with stable identities on redo. Existing manual, posture,
same-side and through-via routing tests remain in the full gate.

MCP checks exercise the cutout variant through both shared tools and independent
clients. Actual egui pointer/control/keyboard tests select endpoints, request the
retained background checked preview, cancel, commit, undo/redo, invalidate a
preview after an outline edit, and refuse Enter after reopening the same project.
UI/MCP action parity checks passed.

Initial CI passed formatting and strict Clippy but exposed an inherited
fabrication UI test race: the queued Export click had no input frame before the
test checked `busy`, so it skipped polling and started the worker during the final
frames. An explicit three-frame pump before polling preserves the five-second
deadline and all assertions. No application behavior or timeout was weakened.
Focused verification and final combined CI cover that synchronization correction.

## Actual production desktop HTTP

The actual executable runs on reserved port **7911**. Its
[desktop metadata](verification/t0085-production-window.json) identifies the real
process. Hyprland allocated a small tiled window; the readable visual below uses
a separately identified inspection host. Existing previews and desktop settings
were preserved.

The [retained HTTP verifier](verification/t0085-cutout-http.py) initialized two
independent MCP clients. Its [25 checks](verification/t0085-native-http.json) passed:

- Immutable complete-session plan with independently expected seven-point path,
  exact off-grid endpoints, derived net/side, one cutout and complete scoped DRC.
- Copper-clearance invariance and the strict one-Nm odd-half-width boundary:
  edge clearance 0.874997 mm escapes at 7 mm; 0.874998 mm escapes at 6 mm.
- Six recomputed committed segments equal the checked path, one event/undo,
  identity-preserving redo, and unchanged stale query/commit from the other client.
- Real peer cutout edits, nonrectangular cutouts, blocked edge corridors,
  endpoints inside a hole, multiple cutouts, and reopened stream guards.

The [saved design](verification/t0085-route.viamind.json) retains two explicit pads,
the substrate cutout and the final committed route. First HTTP plan/commit took
**2.07/4.23 ms**. Five warm commit round trips were
**1.98, 1.96, 14.23, 2.21 and 2.64 ms**, mean **4.60 ms**. These include HTTP and
synchronous complete scoped DRC on this small fixture. Concurrent builds were
active; the nearby host load was 9.32/10.41/13.25. All samples, including the
outlier, are retained. This is observed small-board behavior, without an idle
system or worst-case latency claim.

The first verifier attempt found an omitted optional `outline` key on the blank
project before making any edit. The verifier now handles that omission and was
rerun against the same unchanged production process; no partial attempt is
counted as a passing check.

## Readable native preview

The [native capture](verification/t0085-checked-preview.png) shows the orange
checked detour above the rectangular substrate hole, exact J1/J2 attachments,
updated cutout scope, six segments, one obstacle, 30 graph nodes and the complete
Finish/Cancel controls. The separate connection guide remains visible while the
route is an uncommitted local preview.

The [retained inspection source](verification/t0085-inspection.rs) loads the saved
fixture, removes its same-net tracks before creating the engine, and drives actual
production controls and egui pointer events to create the checked draft. It then
renders that state through native wgpu in a fixed 1600 × 1000 host. This host alone
retains logical egui focus for rendering; it sends no operating-system input.
Production focus-loss cancellation is unchanged. Every native frame asserts the
complete engine session unchanged and nonempty checked points.

[Provenance](verification/t0085-inspection-provenance.json),
[window metadata](verification/t0085-inspection-window.json), and
[runtime assertions](verification/t0085-inspection-runtime.log) identify the unique
inspection executable and actual 1568 × 980 capture. Its production default
0.25 mm grid/width yields escape **y = 7.5 mm, x = 21.5 mm**. The separate actual
HTTP fixture uses 1 mm grid and 0.250001 mm width, yielding **y = 7 mm,
x = 22 mm**. Both retain exact endpoint stubs and complete shared scoped preflight.

## Limits

Outer board and every cutout must be valid axis-aligned four-corner rectangles.
Arbitrary polygons refuse explicitly. Conservative boxes can refuse physically
clear paths. The existing 64 combined boxes, finite graph, work, capture, segment
and retained-operation bounds remain; budget exhaustion refuses without mutation.
Static copper is unchanged. No shove, automatic vias, general-outline routing,
plane/differential routing or physical/manufacturing certification is added.
Scoped checker exclusions still apply; existing unrelated violations can remain.
Conservative stored-endpoint guide proofs remain separate. The strict native
100k-object **60 fps target remains open**.
