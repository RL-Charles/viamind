# Native multi-unit symbol definitions and complete package placement

Ticket: T-0114. Parent: S-034. Initial frozen stack boundary:
`9a6114d5982e47724483ba9b549ce5ea6a87e636` (T110). Only this ticket's
commits will later be transplanted onto the frozen T113 successor boundary.
No Rust implementation precedes independent approval of this specification.

## Public behavior sources

- [KiCad 9 Schematic Editor — creating and editing symbols](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols),
  read 2026-10-11. The manual describes multiple units belonging to one package,
  individual unit names and pin layouts, and selecting a unit for editing.
  Units can have different functions and need not be interchangeable.
- [KiCad 9 Schematic Editor — placing symbols](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#placing-symbols),
  read 2026-10-11. The manual distinguishes placing units from placing another
  complete component instance. Its alternate body styles and shared pins are
  broader than this increment.
- [egui 0.36.2 Id](https://docs.rs/egui/0.36.2/egui/struct.Id.html) and
  [ScrollArea](https://docs.rs/egui/0.36.2/egui/containers/scroll_area/struct.ScrollArea.html),
  used with the independently approved T102/T110 identity and compact clip
  contracts. Stable local editor identity and reachable controls are required.

These are behavior sources only. No source-tool code, library definitions,
icons, component datasheets or existing symbol assets are copied. All geometry
and the quad-amplifier verification definition are authored for Viamind.
The limits, native types and lifecycle below are independent design choices.

## Complete first increment and remaining integration gap

Support creating/editing a detached rectangular definition with 2..32 explicit
units, saving/loading it as a strict native library file, and placing **every
declared unit of one package on one sheet atomically**. This must create a real
four-functional-unit plus separate-power-unit definition, not five unrelated
single-unit symbols that happen to have similar reference strings.

The placed package has one reference, value and optional footprint assignment;
each unit has a stable symbol identity, distinct local geometry and actual pin
endpoints. All package pin numbers are distinct. Definition reuse generates
fresh package/symbol/pin identities without updating existing instances.

This first increment deliberately has no partial package, cross-sheet units,
common/repeated pin, unit swap, interchangeable-unit automation, alternate body,
triangle/art editor, hidden power connection, simulation, or package-to-PCB
aggregation. Existing annotation does not yet understand packages and must
refuse a grouped schematic explicitly. Package-aware annotation and one-package
PCB creation/link/synchronization are named subsequent increments. The original
S034 create-four-units-plus-power workflow is demonstrated; broader integration
and full library-editor equivalence are not claimed.

## Detached native definition

Add a separate `SymbolPackageTemplate` type and format
`viamind-symbol-package`, version 1, extension
`.viamind-symbol-package.json`. Existing `viamind-symbol` version 1 bytes, APIs
and single-unit defaults remain unchanged.

Shared fields: name, value, optional footprint. Ordered units contain a unique
key, display name, explicit functional/power role, positive rectangular width
and height, and ordered pins. A role is descriptive metadata: it does not create
a net or connect a power pin. Each pin has explicit number, name, exact local
integer-Nm coordinate and electrical type from the existing eleven-type enum.
Every unit has at least one pin. Pin numbers are unique across the entire
package. Unit keys are 1..8 ASCII uppercase letters/digits beginning with a
letter; display names use existing trimmed/control-free schematic text rules.

Bounds before secondary ownership/copy: 2..32 units, 1..1024 total pins,
4096 UTF-8 bytes per known string, 256 KiB aggregate definition text and
1 MiB serialized file. Name/unit display/pin text semantic character limits
match existing single-unit definitions; footprint is absent or explicitly
nonempty. Dimensions and coordinates use existing positive/±10000 mm limits.
No float, inferred pin numbering, mirrored pin numbering, implied centroid,
manufacturer data or production footprint default is introduced.

Borrowed admission checks unit/pin counts before visiting a malformed excess
row, then per-field/aggregate text and 1..4096 encoded path bytes before typed
secondary allocations. Unknown fields, invalid enum/type,
missing/empty units, duplicate unit keys or package pin numbers refuse. The
original bounded transport/file buffer is outside the secondary-copy claim.
Raw detached-file JSON also rejects duplicate known object keys; an already
parsed MCP/engine `Value` cannot represent duplicate keys, so no new guarantee
about duplicates discarded by the original transport decoder is claimed.
Tests use 33rd-null-unit and 1025th-null-pin oracles to distinguish count errors
from deserialization of excess malformed rows. The JSON parser's ordinary
recursion refusal remains active; no arbitrary nested extension object exists.

Loading opens nonblocking on Linux, inspects that same descriptor as a regular
file and reads at most 1 MiB plus one byte. NEW writing validates first, stages
and synchronizes a private sibling outside engine/native locks, then publishes
without clobbering under the **latest** active-native lifecycle guard. Path
aliases and a peer Save/Open race cannot replace an active native file. A
racing unrelated destination remains unchanged. No model/history/dirty-state
mutation follows file load/save, including completion on a dirty document.

## Native package membership and compatibility

Add an optional-empty `packages` map to `SchematicContent`, keyed by stable
package UUID. Each package records its identity, template name and an ordered
list of members: unit key/display name/role and symbol UUID. Values and pin
geometry remain in existing `SchematicSymbol`/`SymbolPin` objects. There is one
source of pin geometry and electrical type; group membership is never inferred
from references or suffixes.

All members exist on the containing sheet exactly once; a symbol belongs to
at most one package. Keys, package/symbol/pin IDs are nonnil and globally unique
under existing project identity rules. Package member symbols have the same
explicit reference, value and footprint. A package reference occupies one
global reference slot: only members of that exact package may repeat it.
Other packages and ordinary symbols cannot share the reference. Package-wide
pin-number uniqueness is validated by walking its actual member symbols.

Unpackaged content omits the new empty map and preserves existing native bytes.
Only grouped files require native feature version **11**; new event logs can
carry version 11. Older envelopes/log headers containing group metadata in
their base or before/after sheet deltas refuse, including undo/redo history.
Mixed older feature floors still use their existing maximum rule. Roundtrip,
replay, reset, undo and redo must preserve membership/order and every identity.

Package records count toward sheet/project object and identity budgets.
Before any selected-sheet/global clone, new package strings/member counts are
charged to the existing caller's object/text budget. This applies to selected
sheet snapshots, render exports, graph/ERC capture, wire preparation, annotation
and synchronization snapshots; adding a small visual group must not bypass
their existing limits. New native group deserialization itself bounds member
lists/text. Large unselected board/assembly contents are not cloned by placement.

## Atomic shared commands and observation

Expose matching desktop actions and MCP tools for strict detached load/save,
`place_symbol_package`, `delete_symbol_package`, and a bounded read-only
`query_symbol_packages`. A load/query is explicitly admitted in agent read-only
mode; save/place/delete remain writers. No startup mode is weakened.

Placement requires exact project UUID, initial full event cursor and target
sheet guard, the validated template, an explicit reference and exactly one
placement for each declared unit key. Each placement has exact integer-Nm
position, cardinal quarter turns and optional local X reflection. Count/text,
geometry and global result budgets precede copying the guarded sheet or
generating identities. A whole package must fit the existing target-sheet
10000 objects/pins, 2 MiB text and global 100000 identities/1000 sheets/4 MiB
schematic-text admission. Check transformed pin endpoints and doubled-Nm body
corners exactly, including odd dimensions and overflow/range boundaries.

Only after validation create fresh package, unit symbol and pin UUIDs, add the
group and all units in one Sheet delta and validate the complete result. One
undo removes the entire package; redo restores identical identities/bytes.
Stale/reopened/malformed/oversized/colliding proposals leave full session,
selection, history, native bytes and peer work unchanged. No partial place.

Package deletion requires the same project/full-cursor/exact-sheet guard and
package ID, removes that group and all its member symbols in one delta and
preserves unrelated symbols/wires/labels. Wires formerly ending at removed
pins remain explicit open wires. Undo restores exact endpoints/IDs. Whole-sheet
deletion continues to remove all contents atomically.

Observation has offset and 1..100 limit, deterministic UUID ordering, captured
project/full cursor, total/has-more, package reference and explicit ordered
unit/symbol/pin-number/UUID mapping. Charge borrowed membership/string counts
before copying rows, and cap response members/pins/text. It never modifies
selection or claims electrical/PCB equivalence.

## Existing command and analysis integration

- Generic symbol editing may retain a grouped unit's identity and change its
  pose only. Changing reference/value/footprint/body/pins or deleting one unit
  must refuse with an actionable package-operation message. Ordinary symbols,
  wires/labels/junctions and complete-sheet deletion retain behavior.
  The existing selected-symbol form identifies grouped units and disables the
  unsupported fields/delete action; the dedicated package form offers deletion.
- Existing single-unit source capture/export and PCB link/component creation
  reject a selected grouped unit explicitly. No unit becomes an independent
  package on export or a partial PCB component. Native validation also rejects
  persisted part-to-grouped-unit links; generic/dynamic/batch inputs cannot
  bypass the restriction. Ordinary single-unit targets still work.
- Annotation preview/apply and whole-project PCB synchronization explicitly
  refuse unsupported grouped packages before planning changes. They never
  independently renumber units or accept a partial pin/pad bijection. UI errors
  and capability documentation identify the package-aware follow-up gap.
- Connectivity/ERC still use the actual explicit pin UUIDs/endpoints/types;
  package membership never joins distinct pins, bypasses pin policy or connects
  an isolated power unit. Query results expose package/unit metadata where
  needed to distinguish shared references. Count and result limits remain
  honest. No full ERC, hidden-power or board-annotation claim is added.
- Canvas and SVG/PNG views label units using the shared package reference plus
  explicit unit key/display name, without changing the stored reference to
  invented unique physical components. Existing single-unit rendering remains
  unchanged. Selection stays the selected unit; package deletion is explicit.

## Desktop workflow and actual proof

Add a separate retained multi-unit library/editor form accessible from the
Schematic menu. Shared name/value/footprint, unit list/selector, per-unit
dimensions and ordered pin fields are local until explicit NEW save or
placement. Stable IDs combine editor UUID, unit-row UUID, pin-row UUID and
constant field key; switching units or deleting an earlier row preserves the
other unit's values/cursor and popup identity. A new definition gets fresh local
identities. Unit role never generates or edits pins automatically.

Users explicitly add/edit all five verification units, their distinct package
pin numbers and power electrical types; no copied component library/preset is
needed. A selected loaded definition has a separate complete package-placement
table: one explicit pose per unit and one package reference. Show a local
preview/counterpart unit list and a persistent notice that package-to-PCB
aggregation/annotation are unsupported. Place submits the shared atomic
command, not one command per unit. Show committed group/unit IDs afterwards.

One retained fallible worker handles detached load/save; busy path/reset/unit
mutations are disabled or generation guarded, Close retains the receiver/draft,
and a stale completion cannot replace newer fields. A document/sheet/cursor
change invalidates placement; load/save remain detached. File/mode/draft guards
compose with existing schematic wiring/library/component/cross-probe behavior.
At 640×400 default logical scale, every field/action/error is reachable through
ordinary scroll/keyboard without resizing; footer actions and full effective
paint clips are tested before clicks. Unit/pin lists remain bounded/virtual.

Acceptance uses an authored 14-pin package: four different three-pin amplifier
units (two Inputs and one Output each), plus a two-PowerInput power unit.
Independent expected mappings check all 14 package pins exactly once, one
shared reference, five unit identities and one stable package identity. Actual
explicit wire contacts change only their expected graph component; unconnected
power remains detectable by scoped ERC. No connection is inferred from role.

Required: strict file/budget/negative malformed-row and lifecycle/race tests;
typed/dynamic/batch guarded placement/delete and undo/native/log replay; ordinary
single-unit regressions; explicit annotation/link refusal and graph/ERC/render
coverage; actual compact form/switch/remove/save/place/cancel/stale inputs;
real initialized two-client HTTP plus stdio with session/native invariants;
readable wide five-unit and compact form WGPU captures with retained unique
binary/source hashes, full-session assertions and clean self-close. Run all
four Linux gates, full canonical catalog/drift/workflow and fresh exact-head CI.
Final independent review is required; only a human marks the story done.
