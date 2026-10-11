# Interactive orthogonal schematic wires

## Public behavior references

The [KiCad 9 schematic manual: wires](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#wires)
describes endpoint connections, a wire drawing tool, cardinal segments, posture
switching, and disconnected crossings. Its [grid and snapping section](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#grids-and-snapping)
describes separate grid and connected-object attachment. These public behavioral
references inform an independent native implementation. No external source code,
assets, formats or library geometry are used.

## Shared command

`draw_schematic_wire_polyline` accepts an expected project UUID, literal full event
cursor, exact expected sheet and 2..256 integer Nm vertices. Every adjacent pair
must differ and form a horizontal or vertical segment within native ±10000 mm
bounds. All segments receive fresh identities and are created through one sheet
delta, one transaction and one undo. The command changes neither existing objects
nor board content, selection or sheet metadata. Native persistence and event-log
replay use existing wire/schema support. Typed and dynamic/MCP execution share
validation. Initial transaction cursor semantics apply in a batch.

A stale sequence/stream/project/sheet, malformed path or exceeded result budget
rejects without partial mutation. No implicit junction is inserted: a segment
endpoint on another wire joins under the existing exact connectivity rules;
interior crossings alone remain disconnected. A deliberately placed path corner
at a crossing is an endpoint and therefore joins. The UI explains this distinction.
There is no clearance, ERC, net inference, hidden junction or wire-drag claim.

A bounded read-only snapshot supplies the current sheet and full cursor. Before
copying, require at most 10000 sheet objects, 10000 pins, 4096 bytes per string and
2 MiB aggregate text. The same limits apply to the resulting sheet. Endpoint
index construction is bounded to 30000 entries (pins, wire endpoints, junctions
and label anchors), with a local spatial broadphase for pointer snapping.

Committing also invokes the existing whole-project identity/reference validator.
Before it allocates global identity sets, preflight at most 100000 combined
project, sheet, part, pad, track, via, schematic-object and pin identities,
including proposed wires; at most 1000 sheets; and 4096 bytes per string /
4 MiB aggregate schematic text across all sheets. The preflight walks bounded
counts/text under the engine lock without copying the project. An unrelated
oversized board or sheet rejects the commit unchanged, even if the selected
sheet snapshot succeeds. This is bounded synchronous commit work; no interactive
latency threshold is claimed.

## Desktop tool

An explicit Draw wires action requires an active current sheet and no object
property draft, file operation or conflicting drawing gesture. Capture the
bounded sheet once; maintain a local preview. Attach to exact transformed pin,
wire endpoint, junction or label coordinates within 12 screen pixels before
snapping to a configurable 0.1/0.25/0.5/1/1.27 mm integer grid. T joins may be
placed on grid along a wire interior. Deterministic tie ordering uses distance,
anchor kind, UUID and coordinate. Duplicate coincident coordinates are harmless.
Do not round an attached endpoint to the grid.

Click adds a chosen target plus the cardinal bend needed from the previous point.
Horizontal-first/vertical-first posture is explicit. Enter or Finish wires commits
only the confirmed vertices; hover preview is not committed. Backspace removes
the most recent confirmed click and its inserted bend. Escape/Cancel discards the
whole local path. Middle/Space drag and wheel zoom retain the local draft and
cannot add points. At most 256 resulting vertices are allowed; attempts beyond
that bound are rejected explicitly. Existing selection/edit forms do not consume
wire clicks. A successful commit ends the tool.

Cursor changes make the captured proposal visibly stale and prevent commitment;
failed commitment preserves the local path for inspection/cancel. Sheet/project/
stream/editor replacement cancels old drafts. Cross-probe or ERC navigation must
reject while a wire draft is active. No view action changes model/history. The
shared action catalog and MCP expose the mutation; UI preview remains local.

## Verification and ancestry

Test exact odd-Nm mirrored/rotated endpoint attachment, graph-confirmed pin/T joins
and interior nonjoins, full-session stale/reopen/batch rejection, result budgets,
atomic undo/redo/native/log persistence and real two-client MCP. Include large
unrelated-board and unrelated-sheet validation-budget rejection regressions. Actual egui input
must draw/finish, change posture, backspace/cancel, pan/zoom and reject conflicting
object drafts. Inspect a readable retained Linux native preview and complete fmt,
strict all-target Clippy, workspace tests/doctests and build before submission.

Development initially stacks on T70 cb5db562e3d7438734c50e9dc16c8885099a2c30.
After T70 merges, transplant only T74 commits onto main.
