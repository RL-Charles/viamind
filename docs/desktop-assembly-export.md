# Desktop BOM and placement CSV

Open **File → Export assembly CSV…** in a native design. Choose **BOM** or
**Placement**, then enter an explicit `.csv` output path in an existing directory.
**Export CSV to NEW file** preserves existing files by default.

To replace a prior CSV, explicitly enable **Allow replacing this CSV output** and
use **Export CSV (replace allowed)**. The native project captured as the export
source remains protected even with overwrite enabled. Errors retain the inputs
and existing bytes for correction and retry.

BOM groups exact value/footprint pairs and includes stable component IDs and
reference arrays. Placement exports component origins in global millimetres,
positive X right and Y down, counterclockwise degrees viewed from above. Bottom
components keep their stored transform. CSV preserves Unicode, commas, quotes and
line breaks; import text columns as text in spreadsheets. See the complete
[generic conventions](assembly-exports.md).

The dialog runs one background worker. It can be closed and reopened while work
continues; pending controls are disabled. Completion shows actual path, kind,
component count and source stream/sequence. Later design edits produce a newer
snapshot notice. Export never saves or changes the model, shared selection,
history or dirty state. Application exit can interrupt a worker; check the output
before retrying.

The scrollable window supports compact desktops. Assembly retains its bounded
header-only snapshot and the shared two-snapshot manufacturing admission limit;
busy requests report an error and can be retried after another export finishes.
See [limits](manufacturing-export-limits.md). No supplier identifiers, assembly
centroid/population inference or manufacturer preset is introduced.

Agents use the same `export_assembly_csv` request/schema and bounded writer.
CLI `--export-csv bom|placement` remains available. Rendered SVG/PNG and fabrication
export menus remain separate choices.

CSV bytes are rendered, written and synced before final publication. The final
path checks and atomic rename share the native open/new/save lifecycle guard,
so a file opened or saved as the active project during staging remains protected.
Both the captured source file and the current active file are protected; ordinary
design commands remain available during publication. External filesystem changes
are outside the shared engine lifecycle contract.
