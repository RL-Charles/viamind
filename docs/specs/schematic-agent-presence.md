# Explicit sheet-scoped schematic agent attention

Ticket T-0105 · Parent S-026 · Depends on T-0103.
Proposed for independent public-source review before implementation.

## Public behavior and scope

[Microsoft Live Share coediting](https://learn.microsoft.com/en-us/visualstudio/liveshare/use/coedit-follow-focus-visual-studio-code)
describes collaborator cursors, selections and participant flags. The
[KiCad9 schematic editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html)
describes separate schematic sheets and the sheet navigator. Reviewed2026-10-11
for behavior only. Viamind uses its own protocol and drawing, with no copied
implementation or assets. Extend existing explicit PCB attention to one
explicitly identified schematic sheet. No hierarchy import, inference, following
another user's camera, object lock or collaborative document branch is added.

## Backward-compatible bounded protocol

Keep the three existing MCP tools and their trusted connection ownership.
`set_agent_presence` gains optional `surface`, with strict tagged objects:

- Omitted or `{"kind":"pcb"}` means the existing PCB canvas.
- `{"kind":"schematic","sheet":"UUID"}` selects an existing native sheet.

Null, unknown kinds/fields, missing schematic sheet and PCB objects with a sheet
field reject. Existing required project/stream, cursor, selection and working
label fields and bounds remain unchanged. An omitted surface remains valid for
all existing PCB callers. Returned markers omit `surface` for PCB attention, so
existing PCB response shape remains unchanged; schematic markers include it.
Actual canonical MCP schemas/descriptions must be regenerated from the full
application. No new tool or native/event format version is needed.

Each trusted connection still owns at most one marker across both surfaces.
A successful update changes its surface atomically and renews the same30-second
lease. The global32-participant limit spans both PCB and schematic attention;
changing an existing participant's surface at capacity remains allowed.
Transport identity, display-name sanitization, control-free80-character/256-byte
working label, integer-Nm±10000mm coordinates, exact post-lock monotonic expiry,
no peer eviction, stream guards and read-only startup admission stay unchanged.
Clear removes only the caller's marker independent of its surface. Query shows
all bounded markers with their current project/full cursor; observation and
rendering do not renew leases.

For schematic attention, `selection` accepts at most64 distinct existing symbol,
wire, junction or net-label IDs in the identified sheet. IDs belonging to PCB,
another sheet, sheet itself or nested pins reject. Validate the borrowed Value
array count and scalar/text bounds before secondary typed allocations. Lookup
uses the identified sheet's maps, without scanning other sheets or cloning any
sheet/project. An empty existing sheet accepts an empty selection and a cursor.
Missing sheet or invalid IDs leave prior attention unchanged, including its
surface/content/deadline.

Queries remove selected IDs whose objects have disappeared, as existing PCB
attention does; stored IDs remain so undo may reveal restored objects. A deleted
sheet prunes its entire marker on the next observation/update, without a
renewal; undo does not recreate a pruned lease. No project edits, history,
selection, dirty status, saved bytes or event append result from pruning.
New/Open/reopen invalidates every surface through the existing stream change.
Ordinary edits, undo and changing the human's visible sheet do not invalidate
other existing sheets' leases.

## Native overlay and unchanged human interaction

PCB drawing ignores schematic markers. The schematic canvas draws only markers
whose sheet UUID matches its current visible sheet. Changing the visible sheet
never edits attention or follows an agent. Keep connection palette/name suffix,
wrapped labels, cursor arrow, clipping and noninteractive drawing consistent
with PCB. Retain at-most250ms repaint requests while attention exists and the
existing transient notifier; these requests are not a frame-rate guarantee.

Use the existing schematic camera and already borrowed displayed content; no
additional project/sheet clone or unbounded second object traversal. Selected
symbols use transformed body-corner axis-aligned bounds (excluding pin text).
Selected wires use endpoint axis-aligned bounds expanded for visibility.
Junctions and labels use small screen-space rings around their anchors (label
text is not its selection geometry). At most32×64 bounded map lookups occur.
Hidden/offscreen geometry is skipped/clipped. Empty sheet cursors still render.
No hover/input/focus target, camera movement, shared PCB selection or schematic
local selection/draft change is introduced. Human clicks, editing and undo
continue through existing shared commands.

## Verification before submit

- Public-source specification independently approved before implementation.
- Existing omitted/default PCB request/response shape, bounds, expiry, same-name
  ownership, capacity and read-only tests continue passing.
- Two actual initialized HTTP clients choose different surfaces/sheets, switch at
  capacity, cannot spoof owners, reject foreign-sheet/nested-pin IDs and invalid
  surface forms atomically, and preserve complete session/status/native bytes.
- Deterministic lease/lifecycle tests cover deleted sheet pruning, object-delete
  filtering and undo, same-UUID reopen, no query renewal and unchanged locks.
- Actual egui tests show only current-sheet markers, PCB/schematic separation,
  readable compact clipped labels, empty-sheet cursor, transformed symbol/wire/
  anchor selection geometry, and human edit/undo without attention mutation.
- Actual full application stdio verifies generated catalog and read-only writer
  refusal with observation admitted. Existing isolated agent workflow passes.
- Linux fmt, strict workspace all-target Clippy, workspace tests/build, exact
  combined artifact-head CI and independent implementation review pass.
- Durable provenance identifies source, binary hashes and owned production
  process; readable compact/wide self-closing native WGPU captures check full
  session/native invariants and fsync NEW pixels. Distinguish direct seeded
  inspection, synthetic egui input, production transports and OS input. No
  presented-frame or cadence claim.
