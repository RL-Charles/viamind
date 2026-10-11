# Generic assembly CSV exports

Public sources consulted before implementation:

- [RFC 4180 section 2](https://www.rfc-editor.org/rfc/rfc4180#section-2):
  uniform CSV records, quoted delimiters/newlines, doubled quotes, CRLF records.
- [JLCPCB placement requirements](https://jlcpcb.com/help/article/pick-place-file-for-pcb-assembly):
  reference, centroid X/Y in mm, board side and counter-clockwise degrees.
- [JLCPCB BOM requirements](https://jlcpcb.com/help/article/bill-of-materials-for-pcb-assembly):
  component references, value, package and sourcing information.
- [PCBWay assembly requirements](https://www.pcbway.com/assembly-file-requirements.html):
  BOM and component placement information; their centroid list includes SMT only.

## Contract

Export an immutable shared-engine snapshot; export never changes history, saved
baseline or selection. An MCP result includes the exact snapshot cursor so later
edits can be detected. File I/O runs after the engine lock is released.

Two **generic Viamind** UTF-8 CSVs, with a header and CRLF records:

- `bom`: `Designators,Component IDs,Quantity,Value,Footprint`. Group by exact
  `(value, footprint)` text; groups ordered lexicographically by that tuple.
  Designators and UUIDs within each group are ordered by designator then UUID;
  designators are a JSON string array within one CSV field (unambiguous even for
  names containing delimiters); component IDs are a space-separated UUID list
  in matching order. BOM includes every placed component on either side.
- `placement`: `Designator,Component ID,Value,Footprint,X (mm),Y (mm),Rotation (deg CCW),Side`.
  One row per component ordered by designator then UUID. Positions are **part
  origins**, in the document's global document frame (positive X right, positive
  Y down on the desktop), never automatic board-bound offsets or inferred pad centroids. Rotation
  is the stored placement transform normalized to `[0,360)`, counter-clockwise
  as viewed from the top; bottom components retain global coordinates and stored
  rotation without a second reflection. Side is `Top` or `Bottom`.

Millimetres are formatted exactly from integer nanometres with six fractional
places; degrees from integer millidegrees with three places. No float round-trip,
locale separators, generated timestamps or platform-dependent record endings.
Text values remain unchanged; CSV delimiters, quotes, CR and LF are escaped.
Empty projects produce header-only files. Spreadsheet imports should treat text
columns as text, including footprints with leading zeros.

The CLI `--export-csv bom|placement --open INPUT --output OUTPUT [--overwrite]`
exports and exits without opening a desktop or MCP transport. The shared MCP
`export_assembly_csv` accepts `kind`, `path`, optional `overwrite=false` and
optional `expected_cursor`; stale guards reject before writing. File publication
uses a unique sibling temporary, sync, and atomic no-clobber publication unless
replacement is explicitly requested. Missing parent directories are errors;
failed writes preserve existing destinations. Active native files are protected
from being replaced by CSV output, including equivalent relative paths.

## Deliberate scope

This is a useful generic interchange format, **not a manufacturer upload preset**.
The model currently lacks supplier/MPN mappings, population exclusions, mounting
technology and an independently defined assembly centroid. We neither invent
those values nor label all components SMT. Supplier presets and their actual
upload acceptance remain S-074 follow-up checks. Fabrication geometry, routing,
Gerber and assembly readiness are separate work.
