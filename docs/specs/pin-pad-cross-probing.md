# Stable pin and pad cross-probing

## Public behavioral source

The [public PCB editor manual, cross-probing section](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#cross-probing-from-the-schematic)
describes bidirectional symbol/pin and footprint/pad probing, optional centering,
and explicit navigation. This independent implementation uses only that public
behavior and Viamind's existing native stable component links. No external code,
assets, or proprietary data are used.

## Read-only mapping

`query_linked_pin_pads` accepts one existing schematic symbol or board part UUID,
an offset and a page size of 1..100 (default 100). It captures one immutable engine
snapshot under one read lock and returns the project UUID and full event cursor.
The component's stable `Part.schematic_symbol` link is authoritative; designators,
values, net names and geometry proximity never establish a link. The existing
component query's identity and duplicate-link checks apply.

Pins and explicit rectangular SMD pads are paired by exact, case-sensitive,
nonempty terminal number. Numbers must be unique within their component. A whole
component requires equal pin/pad number sets before any terminal is navigable.
Missing numbers remain separate actionable rows; no partial bijection is implied.
Unlinked components, unresolved links and legacy graphical preview pads have
explicit coverage statuses. Empty terminal sets are reported as empty, not as an
electrical success. Results sort by terminal number, then stable IDs; pagination
is deterministic. Pin names are descriptive only. This is physical identity
mapping, not net connectivity, routing continuity, ERC or board annotation.

Before allocating identity sets or copying strings, reject more than 10000 combined
sheets, symbols, parts, pins and explicit pads. Inspected references, pin names and
terminal numbers are limited to 4096 bytes per string and 2 MiB total. Nil,
duplicate or key-mismatched inspected IDs and duplicate/invalid terminal numbers
are errors. Unused schematic objects, board copper and unused value/footprint/net
strings are neither copied nor claimed to be validated. Queries cannot mutate
the project, undo history, dirty state or shared selection.

Schematic pin endpoints use the existing exact mirrored quarter-turn transform.
PCB pad centers use the exact bottom-X reflection/quarter-turn transform. Both
world coordinates and local input coordinates must be within ±10000 mm. Arbitrary
part angles remain valid model geometry but are explicitly excluded from this
initial integer-coordinate navigation. Overflow or unsupported coordinates yield
non-navigable rows with an explanation, never rounded attachment coordinates.

## Desktop ownership and freshness

The linked-component panel offers an explicit terminal inspection action. One
retained background worker handles a bounded page at a time; cancellation and
replacement retain the receiver until completion and discard stale results.
Context includes project UUID, full event cursor, selected component, source
editor and source selection. A document replacement, peer edit or source selection
change removes old highlights and prevents old navigation, including reopening the
same project UUID with a new event stream.

Terminal row selection and highlights are local view state. Shared board component
selection and schematic component selection remain unchanged. Explicit navigation
centers the destination on the exact endpoint, retaining a useful minimum zoom;
it rechecks current project, cursor, stable component and terminal IDs. Navigation
rejects active file/property/placement/routing/wire edits and hidden destination
copper layers without changing layer visibility. A selected pad is marked only
on visible copper; schematic endpoint highlights are confined to their sheet.
Neither row selection nor navigation changes design/history/dirty/shared selection.

The same bounded query is available through the UI action catalog and MCP. Initial
coverage is explicit SMD pads, stable linked native symbols, exact quarter-turn
geometry and one component at a time. Hierarchical pin mapping, duplicate pad
numbers, arbitrary-angle exact centers and net cross-probing are deferred.
