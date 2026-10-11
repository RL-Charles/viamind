# Export an inspection image from the desktop

Open a project, then choose **File → Export rendered view…**.

1. Choose **Board view** or **Schematic sheet**. For a schematic, choose the specific sheet in the chooser. The chooser lists the first 128 sheets with bounded names; CLI and MCP accept any valid explicit sheet ID.
2. Choose **SVG** or **PNG** and type a **new output file** ending in `.svg` or `.png`. The parent directory must exist. Changing format leaves your typed path intact; mismatched extensions are explained.
3. Choose the drawing layers and pixel dimensions. Auto-fit includes the selected supported geometry plus the specified millimetre padding. Enable **Explicit crop** for a rectangle in model millimetres, X right and Y down. Decimal fields accept up to six places (one nanometre).
4. Click **Export snapshot to NEW file**. Existing files, symlinks, directories and the active native project are preserved. There is no overwrite option.

The export captures an immutable snapshot and runs validation, copying, rendering and disk I/O in one background worker. You can continue editing. Closing the export window keeps its current job running; reopening it shows progress or the result. A second desktop export waits until the first finishes. Closing the application can interrupt an unfinished job; check its destination before retrying.

The result shows the published path and source snapshot. If edits occurred after capture, the window explains that the design has changed. If edits occurred before the worker captured its guarded snapshot, export fails and asks you to retry. Errors preserve the form for correction. Exports do not change design contents, selection, dirty state, native path or undo history.

## Supported content and limits

Board inspection renders explicit outer copper pads, straight tracks, through vias, and polygon outline/cutouts. Legacy footprint previews are omitted and their count is reported. Inner copper, pours and fabrication certification are excluded. See [board views](board-views.md).

Schematic inspection renders one selected sheet's explicit symbol bodies, pin stems/endpoints, wires, junctions and scoped labels. The bundled font becomes SVG outlines; unsupported glyphs fail explicitly. Hierarchy and electrical certification are excluded. See [schematic views](schematic-views.md).

Both formats use the existing production renderers and protected publication APIs. Dimensions are 1–4096 pixels per side. PNG and schematic SVG are limited to 4,194,304 pixels; SVG output is at most 16 MiB and PNG at most 8 MiB. Existing geometry, aggregate text, glyph and floating-point precision limits still apply and produce readable failures. This window does not export PDF or a whole multi-sheet project.

## Agent parity

The desktop action catalog and MCP use the same typed request schemas for `render_board_view`, `render_board_png`, `render_schematic_view` and `render_schematic_png`. They call the same bounded snapshot/render/write APIs. Desktop results show metadata/path/cursor; MCP also transports the SVG or trusted PNG image content. CLI equivalents are documented in the view guides.
