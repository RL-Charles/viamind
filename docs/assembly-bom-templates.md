# Populated assembly BOM templates

The assembly CSV exporter provides two restricted BOM layouts derived from public
manufacturer templates. Both include populated components only. Existing BOM,
placement, sourcing BOM and saved output jobs remain all-component inventories.

| Format | Public sample layout | Required explicit sourcing |
| --- | --- | --- |
| `catalog_bom` / Catalog BOM | JLCPCB four-column BOM | Supplier `LCSC` or `JLCPCB` and capital C followed by digits as supplier stock number |
| `assembly_bom` / Assembly BOM | PCBWay nine-column BOM | Manufacturer part number; manufacturer name is optional |

Enter sourcing with the selected component's properties or the shared
`set_part_assembly` tool. Clear **Populate component** to omit that component from
these two formats. Missing required fields on omitted components are allowed.
Identifiers are user input: there is no stock lookup, purchase or part matching.
The synthetic verification fixture's catalog numbers describe no verified parts.

## Export

In **File → Export assembly CSV…**, choose **Catalog BOM** or **Assembly BOM**,
enter a new `.csv` path in an existing directory and select **Export CSV to NEW
file**. The window scrolls at compact sizes, retains its background worker when
closed, and displays the included component count and complete source cursor.
**Allow replacing this CSV output** grants explicit replacement; captured/current
active native projects remain protected by the shared lifecycle guard.

```sh
viamind --export-csv catalog_bom --open board.viamind.json --output catalog.csv
viamind --export-csv assembly_bom --open board.viamind.json --output assembly.csv
```

MCP uses the same schema, capture and writer:

```json
{"kind":"catalog_bom","path":"/existing/directory/catalog.csv","expected_cursor":{"stream_id":"COPY-CURRENT-UUID","sequence":0}}
```

Pass the current complete cursor from `get_session_state`; the illustrative UUID
above must be replaced. The tool is `export_assembly_csv`. A stale cursor fails
before snapshot admission. Successful `component_count` counts included components
for these two formats, with no history or document changes.

## Validation and grouping

Included references must start with an ASCII letter, contain only ASCII letters,
digits or underscores, and be at most 64 bytes. References that collide after
ASCII case folding are rejected; spelling is preserved. This is Viamind's local
interoperability subset. Footprints must be nonempty. Catalog BOM also requires
a nonempty value. Text cannot have control characters or edge whitespace in
these templates; Unicode, quotes, commas and leading-zero packages are preserved.

Catalog supplier spelling is checked ignoring ASCII case; stock numbers require
capital `C` and at least one digit. Arbitrary supplier numbers and manufacturer
MPNs never substitute for catalog identifiers. The local required-identifier rule
is stricter than the optional heading in the downloaded sample, reflecting
inconsistent optionality in the public guidance. Syntax is not stock or validity
verification.

Rows group exact value, footprint and all sourcing fields. References sort
lexically and split after 200 components per row. Assembly BOM quantities count
that row's references; item numbers start at one. No UUID columns are added. The
assembly sample's Type and Instructions cells stay empty: this preserved template
does not read mounting metadata or infer technology from visible pads or footprint
names. A header-only file represents an
empty populated set.

The exact public sample headers are preserved, including full-width parentheses
in the catalog heading and the assembly package heading's trailing space. See
the [public-source behavior specification](specs/assembly-bom-templates.md).

Limits remain 100000 source components, 4096 bytes per component header, 2 MiB
aggregate header/sourcing text, 32 MiB CSV and two retained manufacturing captures
per process. Rejected exports leave files and the design unchanged.

## Assembly preparation remains open

A separately prepared placement file must contain the same populated reference
subset. The current generic placement table intentionally includes all components
and reports component origins; it supplies no verified assembly centroids or
package zero-angle correction. These BOM layouts add no placement format or new
saved-job profile. Supported fabrication output lacks paste/silkscreen files
requested by some assembly services. Manufacturer upload/review, matched parts,
stock, procurement and acceptance remain separate user work; no complete PCBA
release is claimed.

See the [production and native verification record](assembly-bom-templates-verification.md)
for exact artifact provenance, independent CSV parsing and compact desktop input checks.
