# Annotation of explicit native symbol packages

Ticket T-0115; parent S-045; depends on T-0114. This specification precedes
implementation. Start from T114's reviewed frozen foundation 9521f02 and later
transplant only T115 changes after the final T114 boundary.

## Public behavior

- [KiCad 9 Schematic Editor, Reference Designators and Symbol Annotation](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#reference-designators-and-symbol-annotation),
  read 2026-10-11. Component references distinguish components; units have a
  unit designation in addition to the shared component reference. The annotation
  tool supports retaining existing numbers, resetting references, position-based
  order, and starting from an explicit number.
- [KiCad 9 Schematic Editor, Schematic Setup](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#schematic-setup),
  read 2026-10-11. Unit notation is separately configurable. Viamind retains its
  independently specified explicit package/key representation instead of parsing
  unit suffix conventions.

Only behavior is used. No proprietary code or symbol definitions are copied.
The exact owner ordering, guards, bounds and reports below are Viamind choices.

## Supported operation

Extend the existing immutable `preview_schematic_annotation` and undoable
`annotate_schematic` workflow to handle a mixture of existing single-unit
symbols and T114 explicit same-sheet complete packages. Remove T114's grouped
annotation refusal only after these owner-aware paths are implemented together.

Each explicit package is one reference owner. A single-unit symbol outside a
package is one reference owner. Package membership is authoritative: equal
strings do not infer a package, and suffix text does not infer a unit. Every
unit keeps its exact symbol/pin IDs, key, display name, role, geometry, electrical
pin types, value and footprint. Only each affected member symbol's shared
`designator` changes. Package records and all unrelated schematic/PCB data are
identical. This does not implement package-to-PCB aggregation or synchronization,
unit swapping, partial packages, cross-sheet groups, hierarchy, or automatic
placement-time annotation.

## Deterministic numbering

Keep existing ASCII reference grammar and inclusive 1..999999999 start bound,
100-sheet complete explicit/default UUID sheet order, and XThenY/YThenX choices.
A package's ordering anchor is the position of its member with the
lexicographically smallest explicit unit key; keys are already unique uppercase
ASCII. Do not average coordinates or infer a power/functional anchor. Equal
anchor positions break ties by explicit owner UUID. Ordinary symbols use their
own position/UUID. A membership map is built once per sheet; no all-pairs owner
lookup is introduced.

Parse the shared original reference once per owner. In KeepExisting, reserve a
valid existing numeric reference once for a package, preserve every member's
exact reference string, and assign unnumbered owners the lowest unused number
at/above start within their prefix. Numeric aliases differing only in prefix
case or leading zeros conflict across distinct owners and refuse atomically;
they do not conflict among members of one valid package. ResetAll allocates
references to all owners, canonicalizing assigned prefixes to uppercase as the
existing planner does. A package consumes one number, independent of its member
count. Exhaustion refuses the entire plan. Final reference uniqueness is checked
between owners, while all members of each package must still share exactly one
reference. Invalid/overlapping/missing package membership refuses before planning.

## Preview and guards

The preview leaves complete session/history/selection/native status unchanged.
Retain its existing source cursor, complete exact expected sheet map, options,
symbol count and deterministic per-symbol change rows. Package change rows add
optional package UUID and unit key; omit them for ordinary symbols. Report
package count separately from actual unit/symbol count. A changed package has
one row per member so every stored mutation is reviewable; row order follows
owner order then explicit unit key. No generated identities are introduced.

Add optional `expected_cursor` to AnnotateSchematic with the standard
Command::expected_cursor hook. New previews always include their captured full
cursor. Grouped-package commands require it. Existing ungrouped callers may
omit it and retain the previous exact-project/exact-sheet guard contract; this
legacy omission does not authorize grouped changes. Typed, dynamic and batch
execution compare one initial transaction cursor. Changed sequence or same-UUID
reopen stream rejects grouped application before mutation, even when sheet
bytes happen to match. Exact expected_project/expected_sheets are still checked;
there is no rebase/merge of stale proposals.

Shared apply recomputes the owner-aware plan from the guarded original rather
than trusting submitted replacement references. Each affected sheet emits its
normal complete before/after Sheet delta in one atomic transaction and one undo.
Undo/redo restores all members' shared references with identities/membership
unchanged. An already-current/no-change plan creates no design event or history
entry, preserving existing behavior.

## Bounds and ownership

Retain 100 sheets, 2000 actual symbols/units, 10000 combined schematic rows/pins,
256 KiB aggregate schematic text and 2 MiB serialized guard. Explicit package
records and member records count in the existing combined row cap; membership
name/key/display strings count toward the aggregate text budget before any
sheet/candidate clone. Existing per-field semantic bounds apply. Count all raw
known collection lengths before visiting a malformed excess row.

For MCP/registry inputs, borrowed Value preflight checks collection counts,
known strings and serialized total before secondary typed guard allocation.
Original transport decoding remains outside that secondary-copy guarantee;
already parsed Value cannot preserve duplicate discarded transport keys. Typed
Rust callers supply already-owned inputs but still pass the same borrowed
capture/plan bounds before clones. Preview validates/charges source maps while
borrowed before retaining its bounded immutable copy. Use a bounded serialized
counter rather than allocating an unbounded serialized buffer just to measure
it. Shared package validation and global UUID/reference-owner invariants remain
required. No new global restriction is imposed on unrelated board/assembly data.

## Desktop

Use the existing schematic annotation controls and shared MCP commands. Display
actual symbol/unit count, package count and proposed member changes with plain
ASCII separators. Preserve explicit KeepExisting/ResetAll/order/start choices.
The preview table is bounded/virtualized so large proposals do not render all
rows in every frame. Prepare/Apply and explicit Cancel preview remain reachable
in 640x400; changes scroll separately. A stale full cursor disables Apply and
shows that preparation must be repeated. Changing options/start or explicit
Cancel removes only the local proposal. Existing new/open lifecycle reset and
other draft guards remain intact. No approval/security mode is claimed.

## Evidence required

- Independent expectations for mixed singles, two five-unit packages including
  power units, interleaved anchor positions, retained-reference aliases, different
  prefixes, complete sheet order and X/Y tie-breaks. Packages consume one number.
- Preview unchanged-session/status; grouped missing/stale sequence/stale reopen
  cursor refuses; malformed shared references/IDs/member records and exact raw
  count/text/serialized limits refuse unchanged; same-initial-cursor batch checks.
- Actual typed/dynamic/two-client MCP Apply, native/log version11 roundtrip,
  one undo/redo, unrelated board/assembly preservation, and old ungrouped
  omission/geometry/native-byte parity regressions.
- Actual egui Prepare/Apply/options/Cancel/stale/compact/virtualized rows; readable
  native renderer evidence with honest inspection/input provenance and self-close.
- Linux fmt, strict workspace/all-target Clippy, workspace tests/build under the
  shared whole-sequence mutex; actual combined canonical catalog/workflow and
  fresh exact-head CI. Preserve historical source/binary evidence on integration.
