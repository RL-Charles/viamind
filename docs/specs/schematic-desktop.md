# Linux schematic desktop capture

Depends on the native schematic foundation. Public KiCad editor object categories
and connection-point behavior are linked in `schematic-foundation.md`; this UI
uses original rectangular symbols and Viamind commands.

## Workspace

A Schematic tab replaces the board canvas and board property panel. Board camera,
layers and selection remain local to the board. A separate schematic camera pans
with middle drag or Space+drag, zooms about the pointer and fits with Home/fit.
Sheet selection and object selection are local schematic UI state. The shared
engine remains the sole source of persisted data and undo.

## Capture

Create/rename/delete sheets; add/edit/delete symbols, wires, junctions and
local/global/power labels. Forms use millimetres converted to exact integer nm,
cardinal rotation and mirror controls. A symbol's numbered pin rows include name,
local endpoint and electrical type. Geometry and text use the foundation bounds.
Each Apply uses the exact project UUID and original sheet snapshot. Concurrent
edits reject without discarding the draft; Reload explicitly adopts new data.
Project replacement invalidates old sheet drafts, selection and camera fit.

## Drawing

Draw original rectangular symbol outlines transformed by mirror then quarter turn,
visible pin endpoints and pin numbers/names, reference/value text, straight wires,
junction dots and label names/scope. Use model right/down coordinates. Content
bounds include transformed bodies/endpoints/text anchors. The canvas renders
live agent edits and undo. It does not infer an electrical graph or certify ERC.

## Checks

Actual egui input creates/edits/deletes all supported types, stale guards and undo;
mode switching blocks board gestures; malformed fields keep the draft unchanged;
new document cannot accept an old draft; native Linux view shows a regulator-style
circuit. Exact transformed drawing coordinates are checked independently.
