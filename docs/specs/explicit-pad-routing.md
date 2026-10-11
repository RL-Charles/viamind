# Explicit pad endpoint routing foundation

## Public behavioral sources

- [KiCad 8 PCB editor: routing tracks](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html#routing-tracks) describes starting routes from copper objects and selecting destination objects.
- [KiCad 8 PCB editor: grids and snapping](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html#grids-and-snapping) documents object snapping taking precedence over the grid, including pad origins.
- [egui 0.36.2 Response](https://docs.rs/egui/0.36.2/egui/response/struct.Response.html) documents pointer interactions used by the local preview.

These public behaviors motivate this independent contract. No upstream implementation,
assets, or proprietary code is used. This increment extends the existing
[straight polyline foundation](interactive-polyline-routing.md).

## Shared identities and exact coordinates

`query_route_endpoints` is immutable and returns one source cursor/project identity,
UUID-ordered explicit pad rows, bounded pagination and explicit unsupported reasons.
Rows identify part/pad UUIDs, designator/number, exact case-sensitive assigned net,
outer copper side, representable integer-nanometre center and presentation-only rectangle corners. Legacy graphical
preview pads are excluded. Missing/unassigned nets and arbitrary part angles are
reported as unsupported attachment targets rather than silently inferred or rounded.
Queries are bounded before capture by 100000 board structural objects and 2 MiB
board text/512-byte labels (route net at most 256 bytes); unrelated schematics/project text are not copied.

A shared checked integer transform mirrors local X on the bottom side, applies a
0/90/180/270-degree counterclockwise turn in model Y-down coordinates and adds part
translation. Normalized quarter turns only are supported. Arithmetic overflow or
centers outside the existing copper coordinate range reject; no floating-point
query coordinates are converted back into supposedly exact model endpoints.

`route_pad_polyline` requires project identity, exact initial session cursor,
`start` and `target` persisted part/pad references, positive integer grid/width,
zero to 1023 intermediate integer vertices, and optional scoped DRC rules. It
resolves both references from the guarded live candidate, derives net and layer,
and requires distinct explicit pads with the same nonempty assigned net and
outer copper side. A command cannot override those derived properties.
First/last coordinates are the exact resolved pad centers, even off the chosen
grid. Only intermediate vertices must lie on that grid. Adjacent coincident
points, unsupported transforms, invalid or removed pads, net/side mismatch and
stale stream/sequence/project reject every segment. Coincident endpoint centers
reject as a zero-length route, without claiming electrical connectivity.

The command shares T65 bounded board-only scoped copper/outline preflight,
incomplete-result rejection, proposed-track issue checks and one atomic
transaction/undo. Initial cursor semantics remain identical for typed, dynamic
and grouped execution. Existing unrelated violations may remain. Failed
preflight publishes no tracks, history changes or session events.

## Desktop interaction

Board → Route pad connection opens local width/grid/clearance settings. The
canvas then picks visible explicit pads or visible conservative connection guides.
Picking a pad sets the start; the next compatible pad can become the target.
Picking a guide selects its persisted start/target references in deterministic
UUID order, deriving the same net/side checks. Unsupported targets remain
visible with an actionable reason and never become anonymous grid points.

Pad hits use actual transformed rectangle geometry; nearby small-pad origins may
use a documented six-screen-point tolerance. Guide hits use six-screen-point
segment distance only when both corresponding endpoint copper layers are visible.
Nearest screen distance wins, with stable part/pad UUID tie breaks. Hover
highlights the chosen target and identifies pad/net/layer. Hidden-layer pads,
legacy previews and invisible guides do not intercept routing clicks.

After selecting endpoints, empty-space clicks append grid-snapped intermediate
vertices. Orange preview includes the exact off-grid start and target; it is
unchecked until commit. Backspace removes the latest interior vertex, Escape or
Cancel discards locally, and Enter/Finish invokes the shared command. Changing
endpoints is an explicit local operation. A rejected commit retains the draft
and error; stale edits require cancel/restart. Existing modal, focus, document,
workspace, selection and placement guards apply. No local preview mutates the
shared document or selection.

## Limits and verification

Only quarter-turn rectangular explicit SMD pads on one common outer layer are
attachment targets. No routed vias, track-end attachment, arbitrary-angle
attachment, automatic net inference, posture constraints, push/shove, walkaround,
obstacle avoidance or electrical continuity analysis is added. Connection guides
remain conservative pad-only estimates after routing; this action does not claim
a guide is electrically completed or suppress it. Scoped DRC exclusions remain.
Synchronous preflight can delay other operations, as documented by T65.

Verify exact signed/quarter-turn/bottom transforms, off-grid endpoints and exact
on-grid interiors; same-net/side validation; stale part/pad/net/rotation edits;
whole-batch rejection and undo/redo; immutable bounded query pagination; actual
pointer/key pad/guide visibility/picking/preview/cancel/finish tests; MCP parity;
Linux fmt/clippy/workspace tests/build; and separately labeled native HTTP/visual
evidence. Parent full routing and strict native 60 fps acceptance remain open.
