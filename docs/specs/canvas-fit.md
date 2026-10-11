# Canvas fit while opening a design

## Sources

- [KiCad 9 PCB Editor navigation](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#navigating-the-editing-canvas): public context for fit, pan and zoom.
- Startup viewport tracking below is an original Viamind behavior, prompted by native verification in T-0033. No external implementation code used.

## Behavior

- Opening a project initially fits the board in the usable canvas with the existing 24-point margin.
- Until the user pans or zooms, refit when the usable canvas size changes. This covers native startup layout settling and window resizing.
- A manual pan or zoom preserves the chosen camera center and scale through later resizes.
- Home or View → Zoom to fit explicitly fits again and re-enables size tracking.
- Design edits and selection changes retain the current camera. A resize, rather than a document change, triggers automatic refitting.
- Camera state is local to the view, outside shared design/selection action parity.
