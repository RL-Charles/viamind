# Populated assembly BOM templates

Ticket T-0092, story S-074. Independent behavioral specification, 2026-10-11.
Base: merged main `80d67c05f4c02cb4ae8ac1898e5ec3c15f49d7d0`.
Root review is required before implementation.

## Public sources and observed template data

Only public documentation and its linked spreadsheets inform these behaviors.
Spreadsheet XML was read independently with Python's standard ZIP/XML reader;
no exporter code, proprietary assets or actual component catalog data is copied.

- [JLCPCB BOM requirements](https://jlcpcb.com/help/article/bill-of-materials-for-pcb-assembly), updated September 9, 2026, names description, reference and package information. Its [linked sample XLSX](https://s3.amazonaws.com/helpscout.net/docs/assets/59f1de7804286313cffbb22c/attachments/60ee4acd9e87cb3d0124d284/Sample-BOM_JLCSMT.xlsx) has the four column names specified below, with the identifier column labeled optional.
- [JLCPCB current export guide](https://jlcpcb.com/help/article/how-to-generate-the-bom-and-centroid-file-from-kicad), updated September 9, 2026, instead includes the catalog identifier in its minimum BOM data. This specification chooses the stricter explicit-identifier contract; it does not claim the public pages are consistent about optionality.
- [JLCPCB component metadata tutorial](https://jlcpcb.com/help/article/how-to-generate-bom-and-pick-and-place-file-in-sprint-layout-6), updated September 9, 2026, identifies LCSC and JLCPCB component numbers as the same catalog identifier and documents a capital C followed by digits. Catalog matching, availability and purchase confirmation remain external processes.
- [JLCPCB preparation recommendations](https://jlcpcb.com/help/article/advice-for-bom-and-cpl-files-preparation), updated September 9, 2026, requires globally unique references, consistent reference case across BOM/placement, and at most 200 references per BOM row. The BOM requirements page additionally warns that production uppercases references. Output preserves entered case but rejects collisions after ASCII case folding.
- [PCBWay assembly file requirements](https://www.pcbway.com/assembly-file-requirements.html) distinguishes consigned and turnkey data. Its [linked sample XLSX](https://www.pcbway.com/img/images/pcbway/Sample_BOM_PCBWay.xlsx?v=1.1), inspected October 11, 2026, marks reference, quantity, manufacturer part number and package columns as required, and has the nine column names below. The broader page lists possible additional fields; the downloaded sample has no distributor identifier column. This increment targets that sample layout only.
- [RFC 4180](https://www.rfc-editor.org/rfc/rfc4180) defines escaping of commas, quotes and line breaks. Existing Viamind UTF-8 and CRLF output is retained.

Manufacturer names identify the documentation/template destinations here; Viamind
product, crate and feature names remain independent. No source vendor logo or UI
asset is used.

## Scope, names and population

Add two values to the existing shared assembly CSV kind catalog:

| Shared kind | Desktop label | Documented template destination |
| --- | --- | --- |
| `catalog_bom` | Catalog BOM | Four-column JLCPCB sample layout |
| `assembly_bom` | Assembly BOM | Nine-column PCBWay sample layout |

These are BOM tables only. No placement, centroid, coordinate, rotation, layer or
mounting-type field is inferred. No model/schema change or sourcing setter change
is required. Existing `bom`, `placement`, `sourcing_bom`, fabrication and saved-job
outputs keep their canonical bytes and population semantics. The saved output-job
profile is unchanged.

Only components whose assembly metadata is absent or has `populate=true` appear.
An explicit `populate=false` component is omitted, with no copper/model change.
The desktop explains this selection. Missing required data on an omitted component
does not prevent export; invalid borrowed metadata and snapshot budgets still use
the existing validation contract. An empty populated set emits a header-only file.
The result's `component_count` reports included components for these two kinds;
the existing three kinds retain their current counts. Documentation must distinguish
this subset from existing all-component inventory outputs and remind the user that
any separately prepared placement file must use the same references/population.

## Required input and local interoperability subset

Validate all requirements before rendering rows or staging an output. Errors identify
the offending component reference and field, without suggesting or filling identifiers.

- Included references must be 1–64 ASCII bytes, start with a letter, and contain
  only letters, digits or underscores. Reject empty references, delimiter ambiguity,
  whitespace and duplicates after ASCII case folding. Preserve accepted spelling.
  This is an explicit Viamind subset for predictable comma-separated reference lists,
  not a claim that every vendor rejects all other spellings.
- Included footprints must be nonempty, have no leading/trailing whitespace or
  control characters, and remain within existing header/text budgets. Preserve
  leading zeros and exact UTF-8 text; never interpret package strings numerically.
- Catalog BOM requires a nonempty similarly bounded/clean value for its description
  field, plus explicit supplier and supplier SKU. Supplier must equal `LCSC` or
  `JLCPCB`, ignoring ASCII letter case only; retain entered metadata unchanged.
  Supplier SKU must be a capital C followed by one or more ASCII digits, within
  the existing 512-byte metadata limit. Reject other suppliers, missing identifiers,
  whitespace, lowercase c or punctuation. Manufacturer and MPN never substitute for
  a catalog SKU. This validates syntax and explicit provenance only, not existence,
  stock, exact part matching or any manufacturer approval.
- Assembly BOM requires an explicitly entered nonempty manufacturer MPN. Its
  manufacturer field is optional, as in the sample. Description/value may be empty;
  when present it must have no control or edge whitespace. Optional supplier fields
  are not mapped to unrelated columns. Type and instruction cells are empty because
  current geometry does not establish mounting type or assembly instructions.
- Metadata retains T87 validation and aggregate bounds. Do not normalize or alter
  source fields. Quotes, commas and Unicode in supported description/package/
  manufacturer/MPN cells are correctly escaped; generic table newline behavior stays
  unchanged even though these template kinds use the stricter local text subset.

## Catalog BOM layout

Exact header order from the linked sample:

`Comment`, `Designator`, `Footprint`, `JLCPCB Part #（optional）`.

Rows contain value, comma-joined references, footprint and explicit catalog SKU.
The full-width parentheses in the identifier header are retained. The optional
wording belongs to the original template; the local required-SKU contract above is
stricter and visible in the UI/docs.

Group by exact value, footprint, manufacturer, MPN, supplier and supplier SKU,
so differing explicit sourcing records are never silently merged. Group order is
lexical by that key; references sort by designator then stable ID. Split groups
into consecutive rows containing at most 200 references. No quantity column or
component UUID column is introduced into this template.

## Assembly BOM layout

Exact header order from the linked sample, including the package header's trailing
space and the asterisks denoting required fields:

`Item #`, `*Designator`, `*Qty`, `Manufacturer`, `*Mfg Part #`,
`Description / Value`, `*Package/Footprint `, `Type`, `Your Instructions / Notes`.

Rows contain consecutive item numbers starting at one, comma-joined references,
that row's component count, explicit manufacturer, explicit MPN, value, footprint,
empty type and empty instructions. Group/order/split rules match the catalog BOM,
including a conservative shared 200-reference row cap. Distinct supplier records
stay separate even though those fields are not printed. Quantity equals the number
of references in that row, including after splitting.

## Shared capture, worker and publication

Reuse the sourcing-aware immutable AssemblySnapshot and shared two-slot admission.
Before cloning, validate the existing 100000-source-component cap, 4096-byte
header-string limit, 2 MiB aggregate snapshot text budget including sourcing, and
borrowed metadata. Exclude schematic, pads and copper from the purpose capture.
Validate new template requirements with borrowed input; do not allocate normalized
reference sets/keys before the existing count/text preflight succeeds. Any new
reference-set allocation stays bounded by that admitted snapshot. Reject rendering
a template from a metadata-free legacy snapshot rather than losing entered fields.

Reuse the 32 MiB final CSV cap checked before appending escaped bytes and the
existing prepared-file native lifecycle publication guard. Full expected cursor
checks precede admission. Rendering, staging writes and fsync occur outside the
engine mutex; only final current/captured native-path protection and atomic persist
use the lifecycle guard. Default NEW file; explicit overwrite remains available.
Errors, stale guards, rejected input, busy admission or collisions leave document,
history, cursor and destination unchanged. No stage files remain after failure.

CLI help/parser, MCP schema and the desktop catalog expose the same two shared
kinds. Extend the existing retained background worker; preserve hidden polling,
duplicate prevention, disabled pending inputs, compact vertical scrolling and
result cursor/newer-edit messaging. Use wrapped or separate format-choice rows so
all choices and Export remain accessible at 640×400. No new independent exporter,
worker pool, lock held over await or output-job profile is introduced.

## Verification

- Independently parse exact headers and all CSV rows, including Unicode, commas,
  quotes, leading-zero packages, absent metadata and explicit DNP exclusion.
- Check required fields, supplier/SKU syntax, duplicate/case-colliding references,
  delimiter ambiguity, group ordering, supplier differences and 200/201-reference
  splits with exact quantities. Show empty populated set is header-only.
- Reject invalid/overbudget borrowed data before snapshot copies and publication;
  maintain cancellation/admission/native protection coverage through the shared path.
- Compare old BOM/placement/sourcing/default-job/fabrication bytes before/after the
  new kinds. No source metadata mutation or new native version is permitted.
- CLI and two actual MCP clients produce exact same accepted bytes; stale/full
  cursor, collision, explicit overwrite, active native protection and unchanged
  complete session are covered. Retain durable production reports and exact binary
  provenance, with independent Python CSV parsing and synthetic identifiers only.
- Actual compact desktop input selects each new kind and produces those same bytes;
  pending/hidden/error/recovery/result messages remain readable. Capture a readable
  native inspection window with production versus wrapper provenance explicit.
- Run Linux fmt, strict all-target workspace Clippy, workspace tests/build and final
  combined CI; obtain independent review before merge.

## Open acceptance and limitations

Offline template matching is not supplier lookup or manufacturer acceptance. Actual
upload, component selection, procurement and ordering are separate user actions.
The current component-origin placement lacks validated centroids and package zero
orientation. Supported fabrication output also lacks paste/silkscreen files requested
by some assembly services. No complete PCBA release, vendor-certified geometry,
inventory/price/stock lookup, automatic substitution, component mounting-type
inference, assembly variant or vendor placement format is claimed. S-074 manufacturer
acceptance remains open.
