# Export assembly CSV

Export a saved design without starting a desktop window or agent server:

```sh
viamind --export-csv bom --open design.viamind.json --output bom.csv
viamind --export-csv placement --open design.viamind.json --output placement.csv
```

Output parent directories must already exist. Existing outputs are refused;
append `--overwrite` to replace a previous CSV. CSV output cannot replace the
input native project. A failed write leaves the existing destination intact.
Each output is published atomically from a unique temporary file. These commands
return a nonzero exit status on invalid input, collision or I/O failure; messages
go to stderr and stdout stays empty.

To export unsaved changes from a running desktop or headless session, call the
MCP tool `export_assembly_csv`:

```json
{"kind":"placement","path":"/home/me/design/placement.csv","overwrite":false}
```

For a guarded export, obtain `get_session_state` and include its `cursor` as
`expected_cursor`. The tool rejects an outdated cursor before creating a file.
The result's `exported_cursor` identifies the exact snapshot written. Editing may
continue while the file is written. Exports preserve selection, undo history and
native dirty state; exporting does not save the native document.

## Generic file conventions

Both outputs are UTF-8 CSV with a header, CRLF record endings and quoted commas,
quotes and line breaks. Import text columns as text in spreadsheets to preserve
values and footprint names such as `0603`. Fields are exported verbatim.

BOM groups exact `(value, footprint)` pairs, ordered by value then footprint.
Each row contains `Designators`, `Component IDs`, `Quantity`, `Value`, `Footprint`.
The designator field contains a JSON array such as `["R1","R2"]`, which preserves
arbitrary reference text unambiguously. Its space-separated UUID field has the
same ordering. Component identifiers make it possible to reconcile rows with the
native project even after a reference rename.

Placement has one row per component, ordered by reference then UUID:
`Designator`, `Component ID`, `Value`, `Footprint`, `X (mm)`, `Y (mm)`,
`Rotation (deg CCW)`, `Side`.

Coordinates are component **origins** in the document's global frame, positive X
right and positive Y down on the desktop. They are exact to 0.000001 mm.
Rotation is counter-clockwise in the top view, normalized to `[0,360)` with three
decimal places. `Top` and `Bottom` share this global frame: bottom coordinates and
stored rotations are preserved, without a second reflection. No board bounding
box or pad centroid is substituted for the component origin.

These are generic Viamind interchange files. Manufacturer presets need sourcing
metadata, assembly centroids, population exclusions and mounting technology that
are not yet in the model. Manufacturer upload acceptance and fabrication outputs
remain separate work. See [the behavioral specification](specs/assembly-csv.md).

The CLI interoperability test uses Python 3's standard-library CSV parser as an
independent reader; Python 3 is needed to run the complete Linux test suite.
