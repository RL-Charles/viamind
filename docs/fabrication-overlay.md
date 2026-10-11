# Inspect fabrication files over the board

Open the native board, then choose **File → Inspect fabrication files…**.
Enter one to six explicit `.gbr` or `.xnc` paths and select
**Inspect selected fabrication files**. Empty path fields are ignored.

The result lists source hashes, primitive counts, file layers and the captured
document cursor. Enable **Show fabrication overlay** and choose individual files
to compare their geometry with the PCB. Pan, zoom and Home use the same camera
for the design and overlay. Circular boundaries, rectangular apertures, linear
strokes, profile centerlines and drill boundaries have distinct layer colors.
All sides remain in top view; file upward-Y coordinates become native downward-Y.

Negative solder-mask files display their openings. Drill files and Gerber files
without a project coordinate identity explicitly have an unverified common
origin. A matching UUID identifies the coordinate group; it does not prove the
file matches the current board. Each file is read independently. The source hash
identifies the exact bytes that were inspected.

Any design edit, peer edit or same-project reopen clears the overlay. Inspect
again to compare files against the new document state. Closing the dialog hides
the overlay and retains an outstanding reader or completed result for reopening.
Failures preserve the entered paths and clear the previous picture. Inspection
does not change project data, history, selection, dirty state or source bytes.

The shared MCP tool is `inspect_fabrication_files` with `paths`,
`expected_project` and the complete `expected_cursor` from `get_session_state`.
Results include the captured `source_cursor`, ordered file reports, exact source
SHA256 and integer-Nm geometry. Concurrent edits can leave a historical read
result; consumers must compare its complete context before displaying it.

The supported subset covers Viamind's current five metric Gerber outputs and
metric through-via XNC drill file: circular/rectangular flashes, circular linear
strokes, aperture rotation, mask openings and profile centerlines. General CAM
input, arcs, regions, macros, clear objects, repeated images, slots, inches and
unknown semantics reject the whole inspection. This is a visual comparison tool;
it does not certify manufacturing, DRC or geometric equivalence.

Limits are 4 MiB per file, 16 MiB total, 100000 commands and 10000 drawable
primitives across all files. One process-wide reader admits work without waiting;
retry a busy request after it finishes. The dialog scrolls in compact windows.
See the [behavior specification](specs/fabrication-overlay.md).
