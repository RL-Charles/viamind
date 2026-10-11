# Canvas selection

## Sources (public only)

- [KiCad 9 PCB Editor: selection and the selection filter](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#selection-and-the-selection-filter).
- Viamind extensions below are original product decisions for the agent-driven prototype.

## Behaviour

- Click a visible part to select it. Click blank canvas to clear selection.
- Drag left to right to select parts fully enclosed by the box; drag right to left to select parts touching the box.
- Shift adds, Ctrl/Cmd toggles, Ctrl/Cmd+Shift removes. Escape cancels a drag, or clears selection when idle.
- Choose Box or Lasso in the Edit menu. A lasso selects parts whose visible bounds intersect its closed ring.
- Parts on entirely hidden layers cannot be picked. Highlights follow the shared selection.
- Ctrl/Cmd+A selects all visible parts. Middle-drag and Space+drag pan without selecting.
- Selection is transient engine session state, shared between UI and agents. It leaves design undo/redo and saved files intact.
- MCP tools `get_selection` and `set_selection` read/update stable part IDs. Updates reject unknown IDs atomically and wake the UI.
- Deleting or undoing creation of a selected part removes its ID from selection. Redo does not reselect it.

## Acceptance criteria

- Pointer click, box and lasso input updates the engine selection.
- An MCP selection change becomes visible in the next UI frame.
- Hidden layers, modifiers, panning, cancellation and deleted IDs behave as described.
