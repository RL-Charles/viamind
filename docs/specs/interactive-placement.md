# Interactive grouped part placement

Public behavioural sources: [PCB Editor 8 placement, alignment and distribution manual](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html#_placement_and_drawing_operations), [alignment](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html#_aligning_objects), [distribution](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html#_distributing_objects). These describe moving components with local feedback, rotating/flipping and arranging a selection. This independently designed contract uses part origins for alignment/distribution, explicitly labelled in the desktop; footprint-edge alignment and routing-aware dragging remain future work.

## Shared operation

`transform_parts` takes `expected_project`, a nonempty array of exact `expected` Part snapshots (at most 10000 unique IDs), and a tagged `operation`. Validate all expected records, bounds and resulting records before any mutation; a stale member, missing part, duplicate ID or wrong project rejects the whole operation. Each successful grouped operation is one transaction, preserves part/pad IDs and their local pad geometry, and can be undone/redone as a group. No-ops add no history.

Operations: translate by integer nanometre `delta` (no round trip of omitted model coordinates); rotate each part at its own origin by integer `degrees` in -360..360, normalized to 0..360000 millidegrees; flip the mounting side at the same origin; align origins on X or Y to the minimum, midrange centre or maximum coordinate; distribute origins on X or Y with outermost origins fixed. Distribution sorts by coordinate then UUID and preserves that order; integer division rounds positive spacing offsets down by at most 1 nm. Alignment preserves the other axis. Translation checks integer overflow and native command coordinate limits.

## Desktop

An unmodified primary drag starting on an already-selected visible part moves the current selected group. Pointer movement draws local amber copper/silkscreen ghost geometry; the shared document, events, undo and other agents remain unchanged until release. On release commit one guarded translate command. Escape, Space or losing window focus cancels the pending move and preserves selection; cancelling must suppress release/click handling. A concurrent change to any dragged part makes the entire release fail, clears the ghost and shows an actionable error; no stale move is applied. Changing selection during a drag cancels it. Blank-space or modified drags keep existing box/lasso selection behaviour; middle drag and Space remain camera pan.

When the canvas has keyboard attention and no property text field is active, R rotates the selected parts 90 degrees and F flips them. During a move, rotation/flip shortcuts are ignored until the gesture ends. Edit menu offers the same rotate/flip actions and six labelled origin alignments plus X/Y origin distribution. The operation goes through the same engine command exposed automatically over MCP. Modal part/file/pad/outline workflows suspend editing gestures.

## Checks

Use real pointer and key events to prove preview-before-commit, exact group translation, one-step undo, Escape and Space cancellation, stale multi-agent rejection, preserved box/lasso behaviour, keyboard attention, rotation/flip, menu align/distribute and command/MCP schema parity. Engine tests cover group atomicity, IDs/pads, finite integer boundaries, deterministic spacing and no-op history.
