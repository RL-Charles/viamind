# Explicit calibrated assembly placement

Select one PCB component and scroll its **Assembly metadata** section into view.
Choose **Surface mount**, **Through hole** or **Hybrid**, then enable **Explicit
placement calibration** and enter a footprint-local centroid X/Y in millimetres
and a local zero-axis marker in degrees. **Apply assembly metadata** records one
undoable change. **Clear assembly metadata** clears sourcing, population and
calibration together; **Reload** refreshes a stale draft.

The local frame has X right and Y down before any bottom reflection or component
rotation. Centroid coordinates may be outside the footprint body and must fit
±10000 mm. Marker 0 points along local +X and 90 degrees along local -Y. Its range
is 0..359.999 degrees. These values are your explicit calibration; Viamind does
not calculate a body centre or infer package, polarity or tape/reel orientation.
Mounting and calibration can be stored separately, but both are required for each
populated component in the calibrated export.

## Export

Open **File → Export assembly CSV… → Calibrated placement** and enter a NEW `.csv`
path. Explicit overwrite remains available with active native-file protection.
The retained background worker reports the path, included component count and
complete source stream/sequence. Later edits produce a newer-snapshot notice.
Exporting leaves the shared project, selection, history and dirty state unchanged.

```sh
viamind --export-csv assembly_placement --open board.viamind.json --output assembly.csv
```

Agents use `export_assembly_csv` with `kind:"assembly_placement"`, a local `path`
and optional full `expected_cursor` obtained from `get_session_state`.
`query_parts` returns each component's optional metadata. `set_part_assembly`
replaces the complete metadata value; retain existing sourcing keys when editing
calibration. Supply the full original component as `expected` and the project UUID
as `expected_project` to reject intervening changes. For example, the `assembly`
portion of a request can be:

```json
{
  "mounting": "surface_mount",
  "placement": {
    "centroid_local": {"x": 2000000, "y": 1000000},
    "zero_axis_mdeg": 37125
  }
}
```

Coordinates in the shared model request are integer nanometres and the angle is
integer millidegrees. Missing, floating, unknown or out-of-range placement fields
are rejected. `assembly:null` explicitly clears the entire metadata record.

## Generic coordinate convention

CSV coordinates use the model's global origin `(0,0)`, X right and **Y up**, with
six fractional decimal places. Transform the local centroid by reflecting its X
on bottom, applying the existing component counterclockwise rotation in the
model's Y-down frame, then translating by the component origin. Negate the final
model Y for CSV. Both sides use a common top view; there is no board-bounds origin
shift or additional bottom coordinate reflection.

The rotation column is the final direction of the explicit marker, positive
counterclockwise in the common top view, with three fractional decimal places:

| Side | Output marker direction, modulo 360 degrees |
| --- | --- |
| Top | component rotation + local marker |
| Bottom | component rotation + 180 degrees − local marker |

For a component at `(10,20)` mm with rotation 90 degrees, local centroid `(2,1)`
mm and marker 37.125 degrees, the top output is `(11,-18)` mm / 127.125 degrees.
The same component on bottom produces `(11,-22)` mm / 232.875 degrees. A bottom
component at rotation zero with marker zero outputs 180 degrees.

Quarter turns use checked integer transforms. Other angles round the transformed
centroid at the I/O boundary to the nearest nanometre. Output coordinates must
fit ±99999.999999 mm. These are explicit generic conventions, without a claim
that a manufacturer interprets zero or bottom rotation the same way. Confirm
centroid, orientation, polarity and side against the actual assembly process.

## Rows, population and compatibility

The UTF-8 CSV has these fields, with standard comma/quote/newline escaping and
CRLF record endings:

`Designator`, `Component ID`, `Value`, `Footprint`, `Mounting`, `X (mm, global)`,
`Y (mm, up)`, `Rotation (deg CCW, top view)`, `Side`.

Rows sort by designator then stable component UUID. Mounting values are
`surface_mount`, `through_hole` or `hybrid`; side is `top` or `bottom`. Every
populated component is included, across all three mounting types. Components
without metadata default to populated, so they require explicit calibration before
this export can succeed. Unpopulated components are omitted. Empty or entirely
unpopulated designs produce the header only.

Included references require 1–64 ASCII letters/digits/underscores, starting with
a letter; case-insensitive reference collisions reject the whole export. Missing
mounting/calibration or invalid transformed coordinates also reject the whole
export before publication. Manufacturer, MPN and supplier identifiers are optional
for this generic table. Unused sourcing strings and unrelated board/schematic
geometry are excluded from its bounded snapshot.

Existing ordinary **Placement** continues to export component origins with Y down
and stored bottom rotation, including every component. Existing BOM formats retain
their previous exact bytes and population rules. Current saved output-job profiles
keep their fixed ordinary BOM/origin-placement contents; they do not add this new
table. The populated BOM templates and this generic table share population rules,
without becoming a manufacturer-approved assembly package.

Optional mounting/calibration uses native/event schema 9 while present. Sourcing
alone remains minimum schema 7; named-net policy alone uses schema 8. Mixed features
use the maximum requirement. Clearing the last new field restores the remaining
features' minimum version, while historic log snapshots retain their own requirement.
Older projects remain readable and absent fields retain canonical native bytes.

Limits remain 100000 source components, 4096 UTF-8 bytes per header field, 2 MiB
aggregate header text, 32 MiB CSV and two retained manufacturing snapshots per
process. See [the behavior specification](specs/assembly-placement-calibration.md)
and [manufacturing limits](manufacturing-export-limits.md).
