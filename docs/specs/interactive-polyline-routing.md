# Guarded interactive polyline routing foundation

Ticket T-0065, under S-059. Independent behavior from public documentation only.

## Public sources

- [KiCad PCB Editor 8 routing documentation](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html#routing-tracks) describes choosing track width, grid snapping, placing route segments and cancelling routing. These generic interaction concepts inform this independent, smaller scope.
- [egui 0.36.2 Response](https://docs.rs/egui/0.36.2/egui/struct.Response.html) and [InputState](https://docs.rs/egui/0.36.2/egui/struct.InputState.html) define canvas pointer positions, clicks and keyboard events.
- Existing Viamind public model and scoped DRC specs define integer nanometres, straight round-ended top/bottom tracks, net labels, explicit pads/through vias, board substrate rings and exclusions.

## Behavior

The PCB Board menu opens route settings: exact trimmed nonempty net label, top/bottom copper, positive track width and grid, copper and edge clearance. An explicit board outline is required. After settings, a primary click appends a snapped vertex. Movement previews an additional segment locally. Backspace removes the last vertex; Enter commits the fixed vertices when at least two exist; Escape cancels all local geometry. A Finish control also commits, and Cancel discards. A failed commit keeps the local route and displays the error. Preview segments are explicitly unchecked until commit.

Grid snapping starts with pointer coordinates converted once at the UI/model boundary to integer nanometres, then uses checked integer arithmetic. Nearest-grid ties round away from zero; negative positions behave symmetrically. Stored vertices and width are integers. No accumulated screen rounding or float coordinates enter the design model.

The shared `route_polyline` command takes project identity, the exact session cursor from `get_session_state`, integer vertices/width, layer, net and scoped clearance rules. It validates all segments and preflights a candidate project before applying any deltas. All new tracks commit as one transaction and one undo/redo step with stable IDs. Any invalid point, repeated consecutive vertex, stale stream/sequence, unsafe proposal or incomplete preflight rejects the whole edit. The default command cursor hook preserves existing commands. Typed and dynamic dispatch check the same cursor captured at transaction start, including calls within a batch; a concurrent session edit rejects the preview, while earlier deliberate steps in the same transaction are included in the preflight project.

Before cloning, the command checks a 100,000-structural-object board budget (including proposed segments), 2 MiB board text and 256-byte labels; it captures only board geometry and project identity, leaving unrelated schematic/project text untouched. Net input is checked before candidate allocation. The existing bounded DRC checks candidate tracks against explicit pads, tracks and through vias, exact scoped copper/hole rules and explicit outline/cutouts. Violations involving proposed track IDs reject the entire polyline. Existing unrelated violations may remain; incomplete/truncated reports reject. Named same-net copper can touch under the existing DRC semantics. Existing fallback/legacy parts without explicit pads remain excluded and coverage is stated in UI and tool docs. Preview motion performs no DRC; the bounded check runs at commit under the engine lock, so commit latency can block other clients. A later trusted prepared-command path may move that work outside the lock with a final cursor recheck.

File/modal editing, part placement and selection transforms are guarded while routing. Switching documents/workspaces or losing focus cancels local routing. Camera pan/zoom remain local; route points stay in world coordinates. UI and MCP use the same atomic command and expose the same final geometry.

## Scope and verification

This increment adds user-directed straight polyline placement. Push/shove, walkaround, obstacle avoidance, automatic pad/net attachment, automatic electrical continuity, vias and full manufacturing DRC remain outside scope; parent S-059 push/shove acceptance remains open. Existing ratsnest guides remain conservative explicit-pad estimates.

Tests cover signed/tie/overflow grid snapping; exact segment identity and one undo/redo; invalid/stale stream/sequence rejection across typed, dynamic and batch execution; no history/events/model mutation on rejection; scoped copper/outline/cutout collisions and incomplete checking. Actual egui pointer/key event tests verify local preview, snapping, backspace, finish, cancel and modal/document guards. Linux fmt, strict all-target clippy, workspace tests/build, live MCP command checks and native visual evidence gate submission.
