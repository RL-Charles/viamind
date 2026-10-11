# Reuse native schematic symbols

In **Schematic**, open **Native symbol library…**. Select an existing sheet; use
**Refresh library target sheet** after an agent changes the document.

## Load and place

1. Enter an explicit native symbol file path, then **Load symbol template**.
   The preview names the detached definition and its pin count/value. Loading
   leaves the design and its history unchanged.
2. Enter a new, globally unique reference and exact decimal X/Y in millimetres.
   Up to six fractional places represent one nanometre without rounding.
   Choose 0°, 90°, 180° or 270° and optional local mirror.
3. **Place new library symbol** creates one fresh instance with fresh pin UUIDs.
   One undo removes it; redo restores the same identities. Existing symbols,
   wires and linked PCB components retain their data.

An edit, sheet change or same-project reopen invalidates the old placement guard.
Refresh before trying again. Finish or cancel a schematic object draft or wire
preview before placing. File loading and export run on one retained background
worker; closing the window keeps its job. The window scrolls on a 640×400 display.

## Export a committed definition

Refresh the target sheet, choose **Library source symbol**, enter a template name
and a new output path, then **Export new symbol template file**. The parent
directory must exist. Existing files and the native project file are protected.
The result records the complete source stream UUID and sequence; newer edits are
reported separately.

Export stores the committed local rectangular body, default value, optional
footprint name and ordered numbered pins with local coordinates/electrical types.
It excludes instance UUIDs/reference/placement/rotation/mirror, nets, wires and
PCB links. Editing the library file does not update existing instances. Load and
explicitly place another instance to use a revised definition.

The original [regulator template](../fixtures/symbols/original-regulator.viamind-symbol.json)
illustrates the strict `viamind-symbol` version1 JSON format. Files are regular
files of at most1MiB, at most1024 pins,4096 UTF-8 bytes per field and256KiB aggregate
text; existing native character limits also apply. Unknown fields, duplicate pin
numbers and invalid geometry reject. This version supports rectangular single-unit
symbols. Multi-unit packages, vector graphics, external library formats, live
links and updates to existing instances remain open work.

## Agents

Desktop actions and MCP share:

- `export_symbol_template`: guarded source capture and atomic NEW file publication.
- `load_symbol_template`: bounded detached file validation and definition preview.
- `snapshot_symbol_sheet`: one bounded target sheet, project UUID and full cursor.
- `place_symbol_template`: exact guard plus template/reference/position/transform.

Placement requires both the exact expected sheet and full source cursor. Selected
sheets are limited to10000 objects/10000 pins/2MiB inspected text. Before inherited
global identity validation, placement bounds all visited identities to100000,
sheets to1000 and schematic text to4MiB, including the proposed new instance.
Agent undo remains scoped to the connection; human undo uses the shared timeline.

The [public-source specification](specs/native-symbol-libraries.md) records behavior,
limits and exclusions. See [Linux, MCP and native verification](native-symbol-libraries-verification.md)
for exact artifacts, meaningful regressions and reproduction commands.
