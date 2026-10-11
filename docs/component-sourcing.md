# Component sourcing and population

Select one PCB component. Its Assembly sourcing section accepts optional
manufacturer, manufacturer part number, supplier and supplier stock number.
These are your entries; Viamind does not look up or verify identifiers, stock or prices.
Fields permit 1–512 UTF-8 bytes, with no control characters or surrounding whitespace.
Leave a field empty to mark it unspecified. A project permits 2 MiB of sourcing text
and 100000 components with assembly metadata.

**Populate component** means mount that component. Unchecking it keeps the footprint,
copper and connectivity and records an instruction not to mount it. Apply sourcing
is one undoable change; Clear sourcing removes all entries and restores Populate.
Apply protects the original component and document against intervening edits; use
Reload to inspect another agent's changes before applying again. Metadata belongs
to the PCB component and is preserved by schematic synchronization without becoming
a schematic DNP flag.

## Export

File → Export assembly CSV → Sourcing BOM exports every component, grouped by exact
value, footprint, sourcing entries and population. Population is explicit in each row:
`Populate` or `Do not populate`. Missing identifiers are empty cells. References use
a JSON array inside their CSV cell; IDs remain stable UUIDs.

```sh
viamind --export-csv sourcing_bom --open board.viamind.json --output sourcing.csv
```

The default requires a new file. `--overwrite` permits explicit replacement while
protecting the captured and current active native files. Existing desktop CSV
overwrite and source cursor controls apply to this format too.

MCP `set_part_assembly` sets the complete record or clears it with `assembly:null`.
Supply the full original Part in `expected` and project UUID in `expected_project`
to reject stale edits. `query_parts` returns metadata with the normal full Part
records. `export_assembly_csv` accepts `kind:"sourcing_bom"` and an optional full
`expected_cursor`.

**BOM, placement and the current saved output-job profile remain all-component
inventories and do not apply population or sourcing fields.** Their existing bytes
are unchanged. The sourcing BOM is a generic interchange table, not a manufacturer
preset. Component-origin placement does not establish manufacturer centroids.

Native files use schema 7 only while assembly metadata is present; clearing the last
record restores the prior minimum schema. Older projects remain readable and keep
their canonical shape. Sourcing does not change Gerber/XNC or board view geometry.
Assembly variants, alternatives, procurement, substitutions and vendor acceptance
remain outside this increment.
