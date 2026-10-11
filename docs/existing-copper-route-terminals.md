# Route between existing copper terminals

The desktop and MCP can continue a route from an explicit pad, an existing track start/end, or an existing through-via center. Attachments use stored integer nanometres, including off-grid endpoints. Existing tracks, vias and pads retain their geometry and identities.

## Desktop

1. Open **Board → Route existing copper terminals…**. Choose the outer copper layer, explicit width/grid and manual, cardinal or 45-degree posture in Routing settings.
2. Click an existing terminal to start, then a distinct terminal with the same assigned net on that layer. The window shows their exact coordinates and labels. Through vias can be used from either outer face.
3. Add grid vertices/waypoints by clicking; Backspace removes the last. Slash or **Switch terminal posture** changes orientation. The orange preview is local and unchecked; the persistent footer states this at both viewport sizes.
4. **Finish terminal route** or Enter re-resolves identities and checks the whole candidate before adding any copper. One undo removes every new segment. Escape, Cancel, focus loss, incompatible modes and stale session changes cancel the draft.

The compact screenshot intentionally shows the floating controls covering much of the canvas. It proves readable exact coordinates and persistent notice/Finish/Cancel; the wide screenshot shows the route geometry as well.

## MCP

`query_route_terminals` is immutable and returns stable terminal IDs, exact positions, selected faces, assigned nets, unsupported reasons and the full source cursor. Optional exact `net`, `layer`, `offset` and `limit` filters support deterministic paging; a through via has one row per outer face.

`route_terminal_polyline` requires grid-aligned interior vertices. `route_terminal_posture` computes the cardinal/45-degree path from bounded grid-aligned waypoints. Both take selected IDs, explicit layer/width/grid, request rules, expected project and full initial cursor. The shared engine rechecks document minima and complete scoped joint DRC, then commits all segments as one event. A peer edit, reopen, unsupported attachment or incomplete checker result rejects the complete command unchanged.

See the [public behavioral spec](specs/existing-copper-route-terminals.md), [tool reference](generated/mcp-tools.md), and [machine-readable verification](verification/t0106/master.json).

## Supported subset and bounds

Only persisted explicit pad centers, track endpoints and through-via centers are terminals. Pad transforms must be exact quarter turns. Unsupported pads have reasons; legacy preview-only parts contribute no invented terminals. The chosen face and exact assigned net must agree. No midpoint splitting, new via, inferred width, resizing, obstacle walkaround or shove is performed by this workflow.

Borrowed geometry/policy budgets precede allocation/capture, including 100000 terminal rows before filtering, 100000 board objects, 2 MiB geometry text, 512-byte labels and bounded named rules. Pages contain at most 1000 rows. Input visitors bound interiors/waypoints before secondary vectors; output paths contain at most 1024 segments. Unrelated schematic and assembly metadata are excluded from the geometry-only snapshot. Existing route APIs remain available.

The final shared checker retains its documented coverage exclusions. Existing unrelated violations are not certification; new candidate violations or incomplete scoped checks refuse atomically. Orange local geometry is not a checked plan. Cyan conservative endpoint guides retain their existing semantics; no physical continuity or fabrication certification is claimed. The strict native 100000-object 60 fps criterion remains open.

## Verification provenance

The historical complete Linux run at `a265f71d3cc53cc73e8862781bf2d4d2a41aa6f3` passed fmt, strict workspace/all-target Clippy, 759 workspace tests and doctests (0 failures, 1 existing ignored), workspace/inspection builds, 91-tool discovery and 18 isolated agent workflow checks. Focused tests additionally passed 11 engine, 1 two-client MCP and 5 actual egui input cases. Failed earlier fixture/assertion attempts are retained as excluded history, not successful gates.

The retained production executable `44d6f49618670b9b847f89f01cf3a18d56cfa8373d76e84744e3a4a80f347a46`, PID1966500 on7935, passed 35 initialized two-client actual desktop HTTP checks. Six successful small-fixture commit round trips ranged from 1.48 to 2.05 ms under the observed host load; this is neither a large-board timing bound nor a frame-rate claim. This source is retained by the annotated `verification/t0106/native-source` tag.

The final combined parent is T107 `548a014d4b28b02e814036bf2bcc847736c32cf8`, including T104/T105. All 14 own commits transplanted without conflicts. The final combined focused/lint/build/tooling/catalog/workflow evidence and exact binary/source hashes are listed in the master. The only subsequent product change from the historical verified application is the persistent unchecked-preview footer label; engine/transport behavior is unchanged.

Final wide and compact images come from the real native WGPU Screenshot event in an inspection-only host around production ViamindApp. Actual egui pointer preparation selects the independently expected exact five-point path; full session/status and actual native window bounds and prepared control rectangles against the native viewport are asserted every UI pass. The host preserves logical focus, exclusively creates/fsyncs P6 readback, logs actual UI-pass callbacks and self-closes. PNG conversion preserves every RGB pixel and is independently checked. These are native renderer artifacts, not OS-input or compositor-capture proof. The retained production HTTP proof is separate.

Initial historical inspection images omitted the floating controls after cross-context egui memory transfer; they are excluded. A fresh-area host corrected that inspection issue. The final host includes the persistent preview notice and asserts all three footer control/notice bounds. Historical captures are never relabeled as final integrated binaries.

Final exact-head CI and prospective merge-tree equality with the tested combined tree remain required before merge; the master distinguishes local successful gates from remote CI status.
