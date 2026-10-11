# Guarded properties for existing tracks and through vias

Ticket: T-0111. Parent: S-059. Initial stack boundary: frozen T106
`7178e7d7edf94f3e50cbaf06c1c7aefcaa74d5d5`; only this ticket's commits
will be transplanted after its parent merges. No implementation precedes
independent approval of this specification.

## Public behavioral sources

- [KiCad 9 PCB Editor — editing track and via properties](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#editing-track-and-via-properties),
  read 2026-10-11. Existing copper can have explicit sizes and assigned nets
  changed through properties; tracks have a layer, and through vias retain
  front/back span with copper and drill diameters.
- [KiCad 9 PCB Editor — board constraints](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#board-setup),
  read with the approved T97/T101 minima specifications. Enforced minimum
  constraints and user-selected dimensions are separate concepts.

These sources describe behavior only. This is an independent implementation.
No external code, proprietary binaries or UI assets are used. Viamind's
transaction/cursor, integer geometry, bounded DRC and rule contracts below are
its own explicit choices; broader source-tool behavior is not promised.

## User-visible need and supported subset

The current copper form adds or deletes objects. Fixing an existing width,
layer, net, via diameter or drill otherwise replaces an identity. This
increment edits one persisted object while retaining its ID and geometry.

- Track: explicit positive width, top/bottom copper layer and optional exact net.
  Start/end coordinates and identity are read-only and remain byte-identical.
- Through via: explicit positive copper diameter, drill diameter and optional
  exact net. Position, identity and through span remain byte-identical.
- One selected track or via at a time. Unsupported mixed/part selection or
  missing objects produces an actionable refusal. No bulk edit, movement,
  implicit width/diameter inference, automatic net assignment, geometric repair,
  rerouting, new via, shove or physical-continuity certification.
- Existing add/delete/routing/cleanup APIs retain their behavior. No document,
  native file or event schema/version changes are needed.

## Immutable observation and bounded input

Add strict tagged `CopperPropertyId`: `track { id }` or `via { id }`.
`query_copper_properties` reads one exact current object, project ID/full
source cursor and bounded named-net document minima. It does not select,
mutate, add history, clone the board or copy unrelated schematic/assembly
contents. Read-only agent admission explicitly allows this observation.
Unsupported kind, unknown keys, nil/missing identity and invalid or oversized
selected object text refuse before an owned observation copy.

Add `set_track_properties` and `set_via_properties` commands, exposed identically
to UI and MCP. Each requires expected project, full initial expected cursor,
ID and complete expected original typed object, explicit replacement properties
and optional existing DrcInput request baselines. Strict input schemas reject
unknown fields. Expected-object/replacement net visitors bound any secondary
string allocation to 256 UTF-8 bytes; assigned labels must also fit 128
characters, be trimmed/control-free/nonempty. `null` represents unassigned;
empty assigned strings are rejected. No second unbounded Value/object clone.
The existing transport parser's original buffer is outside this secondary-copy
claim. Exact integer Nm fields retain odd values; desktop decimal conversion
must be exact and reject fractional nanometres, overflow and nonfinite values.

## Atomic engine contract

1. The generic initial-cursor hook rejects stale stream or sequence in typed,
   dynamic and batch dispatch. Project identity and exact expected object must
   match the live target; its map key, stored ID and kind must agree.
2. Validate original/replacement geometry, explicit property ranges and request
   rules before candidate allocation. Replacement widths/diameters/drills obey
   existing model limits. The target net's document minimum track width, drill
   diameter and doubled annular-width comparison use the central resolver and
   max with request baselines. They never resize the requested properties.
3. Refuse an exact no-op with no event/history mutation. Otherwise charge the
   existing 100000 structural-object/2 MiB geometry-text/512-byte label and
   named-rule borrowed snapshot budgets before one board-only geometry copy.
   The copy excludes unrelated project/schematic/assembly metadata. A single
   bounded update does not reserve extra persisted copper capacity.
4. Replace the target in that private candidate and run the shared production
   DRC on the exact candidate and captured rules. Outline required. Complete
   scoped checking includes pad/track/via clearance, candidate edge/cutout,
   width, drill, annular and hole constraints. Any violation referencing the
   edited identity, or any incomplete/truncated report, rejects the whole edit.
   Existing unrelated violations may remain. Inherited checker exclusions
   remain explicitly disclosed; success is not full-design certification.
5. Only after success emit one Track/Via delta with before/after under the same
   ID. No partial model/history/status/selection changes on refusal. One undo
   restores every original property; redo restores identical edited bytes and
   identity. Existing pads, outlines, other copper and conservative guide
   algorithms remain unchanged; normal scene/guide derivation sees the edit.

Commit is bounded synchronous work under the existing engine lock. Measure
actual small-fixture native/HTTP latency and disclose it; do not claim an
unmeasured large-board responsiveness bound. No background worker or extra
snapshot clone is required for this single-object property edit.

## Desktop behavior

Add an **Edit selected copper properties** entry for one selected persisted
track/via, plus an edit action from existing paged copper rows. Reuse the copper
editor's busy lifetime so existing routing/placement/outline/file guards remain
consistent; avoid a second unguarded modal state.

Capture the immutable observation once. Show kind/ID, exact read-only Nm
coordinates and current document minima. All property fields are local strings
until Apply; stable input IDs combine object UUID and constant field key.
Apply/Reload/Cancel remain visible outside an inner scroll region at 640x400.
A persistent notice states that Apply runs scoped checks and excluded objects
are not certified. No local geometry is labeled checked before Apply.

Apply uses the shared command with the captured expected object/full cursor.
Refusal keeps typed fields and shows an actionable error. A peer edit or
same-project reopen disables applying the stale capture; explicit Reload
refreshes the current original/cursor and discards local fields. Cancel/Escape
or incompatible mode/file transitions discard the local draft without an
engine command. Closing the property dialog does not delete or replace copper.
Successful Apply refreshes the retained observation to the exact committed
object/cursor and reports scoped success; no second history event is added.

## Acceptance evidence

- Independently approved public specification before code.
- Independent expected geometry/bytes for track and via property edits, odd-Nm
  sizes, equality/one-Nm below rules and half-Nm annular boundaries, exact net
  case/unassigned behavior, stable IDs/coordinates, one undo/redo and guides.
- Foreign copper collision, same-net physical drill spacing, outline/cutout,
  document-minima net/layer changes and incomplete result-limit refusals keep
  complete session/status/selection unchanged. Existing unrelated violations
  and legacy/excluded coverage are honestly distinguished.
- Typed/dynamic/batch stale stream/sequence/expected-object and peer rule edits,
  oversized raw strings/unknown fields, borrowed capture budget boundaries and
  huge unused assembly/schematic regressions. Existing routing APIs still pass.
- Actual egui selection/row entry, local invalid/Apply/Reload/Cancel/undo and
  stale reopen tests; 640x400 footer bounds and stable field identities.
- Full Linux fmt/strict workspace all-target Clippy/workspace tests/build;
  actual initialized two-client production HTTP/read-only tool parity and
  latency evidence; readable wide/compact native renderer captures with clear
  inspection provenance and clean close.
- Actual full-application canonical catalog generation/exact drift check,
  isolated workflow tests and fresh exact-head CI. No strict100k/60fps acceptance
  change or optional blind benchmark repeats.
