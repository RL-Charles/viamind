# Explicit component sourcing and population

Ticket T-0087, story S-074. Clean-room behavioral specification, 2026-10-11.

## Public sources

- [KiCad 9 schematic manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html), symbol properties, field templates and BOM export: custom manufacturer part-number fields, optional DNP exclusion, and grouping by value, footprint, DNP and additional fields are documented behaviors. DNP retains the footprint while indicating that the part should not be mounted. These concepts inform the independent behavior below; no source code or assets are copied.
- [JLCPCB public BOM requirements](https://jlcpcb.com/help/article/bill-of-materials-for-pcb-assembly), updated September 9, 2026: sourcing information accompanies component descriptions, references and packages. This motivates explicit user input; it does not establish acceptance of Viamind's generic CSV.
- [JLCPCB public placement requirements](https://jlcpcb.com/help/article/pick-place-file-for-pcb-assembly), updated September 9, 2026: placement requires metric component centroids and declared side/rotation. Viamind's existing component-origin placement remains distinct from such validated centroids.
- [RFC 4180](https://www.rfc-editor.org/rfc/rfc4180): commas, quotes and line breaks require CSV quoting, with embedded quotes doubled. Existing Viamind UTF-8/CRLF behavior is retained.

## Scope and population contract

Each board component may carry optional, explicitly entered assembly metadata:
manufacturer, manufacturer part number (`mpn`), supplier, supplier stock number
(`supplier_sku`), and `populate` (default true). Missing strings mean unspecified.
No fields are inferred from component values, footprints, schematic names or other
identifiers. There is no stock, price, identifier-validity or vendor approval lookup.

Population is a board-component assembly instruction only. `populate=false` means
do not mount that component; its footprint, copper, connectivity and design identity
remain unchanged. It is not a schematic DNP flag and does not propagate through
schematic/PCB synchronization. Existing generic BOM, placement and saved output-job
profile intentionally remain inventories of all board components and keep their
canonical bytes. Their UI/docs must explain that they do not apply population.
The new sourcing BOM includes every component and explicitly distinguishes Populate
and Do not populate rows. No output silently omits DNP components in this increment.

## Model, compatibility and validation

`Part.assembly: Option<AssemblyMetadata>` is omitted when absent and defaults to
absent on old reads. Metadata uses strict known fields. Four optional strings each
contain 1–512 UTF-8 bytes when present, with no control characters or leading/trailing
whitespace. Unicode, punctuation, quotes and commas are allowed. Identifiers remain
case-sensitive and are never normalized. A complete empty/default record is
canonicalized to absence by the setter; native readers can represent explicit defaults.
Across the project, at most 100000 components may carry metadata and aggregate
sourcing text is capped at 2 MiB. Metadata-free components do not count toward this
new cap. Validate borrowed records before any mutation or model copy.

Native schema 7 is required only when any metadata is present; projects without it
retain their previous minimum schema and byte shape. Event-log version 7 accepts
older logs; lower-version headers reject metadata in both base and intermediate
before/after part records. Validate metadata before native save/load, in commands,
and in every event record, including values later deleted. Invalid input leaves
project, undo/redo, event cursor and files unchanged. Unrelated operations preserve
metadata through their existing part deltas; newly created/imported parts start absent.

## Shared command, query and desktop

Add a typed undoable setter taking part ID, optional complete metadata (null clears),
and optional expected original part/guard in the existing command style. Validate
borrowed input and original record before cloning. The command is registered through
the shared engine/MCP registry and the UI action catalog. Existing bounded `query_parts`
returns metadata in its full part records; validate new metadata before copying a
returned record. An invalid matching record fails the query rather than returning
invalid metadata. Limit remains 1–1000 records (at most 2 MiB new text per full page).

The desktop exposes a selected component's bounded text fields and explicit Populate
checkbox, with clear/apply controls and readable errors in a vertically scrollable
dialog/properties section. Apply uses the same guarded command. Refresh after successful
apply; another agent's intervening edit must reject a stale form. Support compact
640×400 input and undo/redo. Keep routing, placement and file-operation guards intact.

## Generic sourcing BOM

Add shared CSV kind `sourcing_bom` to CLI, MCP and the existing desktop CSV dialog.
Header (fixed order): Designators, Component IDs, Quantity, Value, Footprint,
Manufacturer, MPN, Supplier, Supplier SKU, Population.

Group by exact value, footprint, all four optional strings and population. Absent
metadata behaves as unspecified strings with populate=true. Group ordering is lexical
by that key; references sort by designator then stable ID. Designators retain the
existing JSON-array cell representation; component IDs retain the existing space
separated UUID cell. Population cells are `Populate` or `Do not populate`. Empty
optional fields emit empty CSV cells. UTF-8, CRLF and existing escaping apply. Existing
`bom` and `placement` rendering must remain byte-identical, including after metadata
edits. The current output-job `generic_outer_2layer` profile continues to render only
its original BOM/placement; no new job profile or format is added.

## Bounded captures and publication

Retain existing two-slot manufacturing admission and 100000-component/32 MiB CSV
limits. A sourcing capture validates all borrowed component header strings and metadata
before any copies, with the existing 4096-byte header and 2 MiB aggregate text budget
including the four new fields. Old generic captures copy only headers and exclude
metadata without first cloning it. Kind-aware capture must prevent a metadata-free
snapshot from accidentally producing a sourcing BOM that loses entered fields.

Fabrication/saved-job and board SVG/PNG captures must exclude unused metadata before
copying parts. Introduce a small explicit part/board geometry-copy helper if useful;
do not clone then erase strings. Direct board render calls must normalize metadata
away just as unrelated schematic data is excluded. Reuse existing geometry/text
budgets, integer geometry and output limits. Schematic views remain unrelated.

Publication uses the existing prepared CSV staging and native lifecycle guard, covering
both captured and current active native files and in-flight saves. Default NEW path,
explicit overwrite, cancellation/worker ownership and source cursor behavior remain
unchanged. No mutex is held during CSV rendering or file write/fsync.

## Constructor and capture inventory before implementation

At base 8d77b23, production Part literals are AddPart, schematic-component PCB creation,
assembly header capture, and UI board-scene fixture. Test constructors span doc
board/save-load/determinism/links; engine delta/query/connectivity/copper/sync/routing/
endpoint/via/posture/walkaround; MCP routing/connectivity; UI routing/via/posture/
walkaround; fab assembly and DRC batch/copper. Inventory with `rg -n '\\bPart \\{' crates apps`
again after rebase; the parallel PCB importer will require the new default too.

Unbounded new-string copy risks found before code: `FabricationSnapshot::capture`
and render `board_view_snapshot` currently use full board clones; assembly capture
already constructs header-only Parts; `PartQuery::run` clones returned full records.
Each must follow the purpose-specific rules above. Design mutations and undo deltas
retain validated metadata rather than stripping it.

## Verification and limits

- Old fixtures retain exact native and existing generic CSV/job bytes; metadata projects
  save/reopen at schema 7, clear back to prior schema, and event logs replay/undo/redo.
- Reject malformed/oversized/control/blank metadata atomically; reject old-version
  intermediate metadata events even if the final project lacks it.
- Parse sourcing CSV independently: Unicode/quotes/commas, exact grouping, unspecified
  fields and both population states. Existing header newlines remain supported.
- Direct invalid/large unrelated metadata must not be copied or validated by board
  views, fabrication or old generic assembly captures; sourcing overbudget rejects
  without state changes. New text is validated before clone in query/capture paths.
- Actual compact desktop input, stale agent edit, shared setter/query parity, undo,
  save/reopen and exact native CLI/MCP CSV bytes; retain durable reports and readable
  native capture with honest production/wrapper provenance.
- Full Linux fmt, strict all-target workspace Clippy, tests/build and final combined CI.

Vendor presets, centroids, multiple supplier alternatives, schematic sourcing fields,
assembly variants, stock, purchasing, automatic substitutions and vendor certification
remain outside this increment. S-074 manufacturer acceptance remains open.
