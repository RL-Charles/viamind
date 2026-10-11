# Detached native symbol definition editing (T86)

## Public behavior and scope

Independent behavior from the public
[KiCad 9 symbol editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols):
symbol properties and explicit local pins have editable names, numbers, positions
and electrical types, and a definition can be saved for later reuse. No external
implementation, symbol data or assets are copied. Viamind keeps its existing
strict native rectangular single-unit format and exact coordinate conventions.

T80 supplies bounded detached definitions and guarded placement. This increment
adds creation/editing of those definitions from actual desktop controls. It does
not add multi-unit packages, arbitrary graphics, external formats or implicit
updates of placed instances. The parent quad-op-amp acceptance remains open.

## Local definition editor

From the native symbol library, explicitly create a new definition or copy the
currently loaded definition into a local editor. A new definition begins with a
positive rectangular body and one passive numbered pin. The editable properties
are template name, default value, optional footprint, body width/height and the
ordered pins' number, name, local X/Y and existing native electrical type.
Allow explicitly adding/removing pins, at most1024, with virtualized pin rows.
Coordinate entry uses the existing exact decimal Nm parser; six fractional mm
digits are accepted without rounding, excess precision/scientific/nonfinite
notation rejects. Width/height must be positive and at most10000mm.

The editor owns bounded text fields and one bounded pin vector. Existing native
field character limits,4096-byte individual/256KiB aggregate definition text and
1MiB serialized-file limits remain authoritative. Check borrowed text and pin
counts before constructing/copying a typed template. Never allocate a new row
beyond the cap. Duplicate/blank pin numbers, malformed names, invalid geometry
and excessive text report actionable errors while preserving editable inputs.

Typing, opening, closing, adding/removing rows and validation never mutate the
engine, undo/history, shared selection or existing file. Close/Cancel discards the
local edit without changing the loaded placement template. Keep an unsaved edit
through peer design edits; it is detached from the project. Reopening or creating
another definition requires an explicit action and never silently replaces a
currently open editor.

**Use edited definition** validates then explicitly replaces the local loaded
placement template. It does not place or update any instance. The existing
fresh project/full-cursor/sheet and reference/transform guards still govern
**Place new library symbol**. A peer edit or same-project reopen still requires
refreshing the selected target; no guard is fabricated by the editor.

## Shared NEW file save

Add typed `SaveSymbolTemplate { template, path }` and the shared
`save_symbol_template` desktop/MCP action. Validate the bounded definition and
call the existing atomic NEW writer outside the engine lock, capturing only the
active native path for protection. Return canonical path, name and pin count;
there is no source document cursor because the saved definition is detached.
Unknown request/template fields reject. Existing paths/native documents and
partial file publication remain protected. No overwrite option or native model
schema change is introduced.

The editor's **Save NEW definition file** uses the library's existing single
retained named background worker. Pending save disables editor mutations and
duplicate saves until completion; hidden polling retains its result. Starting a
new edit is disabled during a file operation. Errors, disconnection and spawn
failure recover without clearing local fields. Successful save does not commit
the definition to the design or implicitly update the loaded placement template.
Completion reports the saved path/name/pin count even if unrelated document edits
occur; stale context cannot enable placement. Preserve other file/route/wire/
component/export actions.

Both the library and definition form scroll in a640×400 viewport. Pin editing
uses bounded visible rows with stable per-row widget identity. Add/remove must
not transfer an old widget edit to a different row. All controls have labels
usable by actual egui interaction tests.

## Evidence

Actual desktop tests create an original asymmetric custom definition, edit body,
pin fields/electrical type and exact odd-Nm coordinates, add/remove a pin, save
NEW and load/place it. Verify source/native files and all prior instances remain
unchanged; one guarded placement/undo/redo has fresh stable identities and exact
transformed pins. Invalid precision/duplicates/budgets and publication collisions
reject without losing fields. Cover compact scrolling, retained pending/hidden
jobs, source-context replacement, spawn/disconnection and duplicate suppression.

MCP uses the same typed writer/schema and validates real production save/load/
place plus collision/native/nonregular failures with two independent sessions.
Retain Linux fmt, strict all-target clippy, workspace tests/build, exact copied
production/native proof, readable capture and independent review before merge.
Do not infer successful checks from a failed run. T86 starts from T80 artifact
head564a881; transplant only T86 commits onto merged main before final CI.
