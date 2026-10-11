# Desktop rendered view export

Public-source behavior specification, written before implementation for T-0071.

## Sources

- [SVG 2 coordinates](https://www.w3.org/TR/SVG2/coords.html): viewport dimensions, model coordinates and aspect-preserving fitting.
- [PNG third edition](https://www.w3.org/TR/png-3/): PNG is a lossless raster image format; use the existing transparent RGBA8 production renderer.
- [egui Context](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.request_repaint): context clones share state and workers can request repaint; UI frame callbacks must remain short.
- [Rust thread spawning](https://doc.rust-lang.org/std/thread/fn.spawn.html): owned work can run independently and communicate through channels; Builder::spawn reports thread creation failures.
- Existing independent Viamind specifications: [board SVG](board-svg-views.md), [board PNG](board-png-views.md), [selected schematic views](schematic-rendered-views.md).

## Contract

1. File → Export rendered view opens a read-only export window. The human explicitly chooses board or one schematic sheet, SVG or PNG, selected drawing layers, pixel dimensions, auto-fit padding or an optional rectangular model-space crop, and a NEW destination file. Changing format never silently changes the typed path.
2. Sheet selection uses a bounded list of IDs and truncated names. It does not clone schematic contents or the board. A selected sheet ID remains explicit in the request. Missing/deleted sheets fail visibly.
3. The four UI actions share typed request definitions and JSON schemas with existing MCP render_board_view, render_board_png, render_schematic_view and render_schematic_png tools. Both surfaces call the same bounded production snapshot, render and protected publication functions. No model or history commands are introduced.
4. One desktop export worker runs at a time, including when the window is hidden. The window may be closed and reopened while work continues. Snapshot validation/copy, rendering and file I/O occur in that worker. Polling does not wait for completion. Thread creation/disconnection failures are readable and allow retry.
5. Clicking Export captures a cheap expected event cursor. The worker checks that cursor before capturing selected bounded data and native path under the same engine lock. If the document changed before capture, no file is published. Later edits are allowed: output represents its captured source cursor, and completion explains when the current document has newer edits or a different session. Export does not change project, selection, cursor, dirty state, native path or undo history.
6. Publication requires a nonempty NEW path and uses existing atomic collision/native-file protection. Existing files, directories, symlinks and active native files are preserved. Extension mismatch is explained before starting. Parent directories must already exist. No overwrite option is added.
7. The form exposes dimensions and crop/padding with explicit units. Integer nanometres remain the request format; millimetre text is converted exactly with at most six decimal places. Invalid fields fail without a worker or output. The shared renderer enforces geometry/text/precision/pixel/output limits; these errors remain visible.
8. Completion shows destination, selected view/format, source snapshot and any omitted legacy board previews. The window explains supported outer copper/outline or selected-sheet symbol/pin/wire/junction/label inspection, with no manufacturing/electrical certification. SVG schematic text uses the existing bundled outlined font.
9. Existing production limits and exclusions remain in force. This increment adds no PDF, hierarchy navigation, whole-project multi-sheet printing, fabrication content, new font dependency or external resource loading.

## Verification

Use actual egui input to select targets/formats, fill a NEW path and export a fixture. Compare UI files byte-for-byte with production render APIs and MCP/CLI outputs. Check asynchronous progress using a controlled pending worker, hidden-window polling, duplicate-start protection and actionable failures. Test crop decimal conversion, missing sheet, stale cursor, collision/native protection and read-only state. Run Linux workspace fmt/clippy/tests/build, CI, and fresh native visual inspection without changing production viewport behavior.
