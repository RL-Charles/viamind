# Standalone PCB geometry import

Choose **File → Import PCB S-expression…**, enter a `.kicad_pcb` path and choose
**Preview PCB import**. The form scrolls in compact windows so Apply and errors stay reachable. Preview runs one bounded background job and preserves the
complete native document, selection, history and save destination. Inspect the
per-object fidelity report: each source list has a stable path, source byte offset,
optional source UUID, disposition and reason. Full fidelity is explicitly false.

This initial converter supports front-side, zero-angle footprints with explicit
rectangular SMD pads at exact cardinal angles within -360..360 degrees, straight outer-layer tracks, circular through vias and
polygon `Edge.Cuts` outlines/cutouts. Decimal coordinates are converted exactly to
integer nanometres; nonzero sub-Nm precision is rejected. Pad numbers, native
part/pad/track/via UUIDs and exact net-table assignments are retained. Source edge
UUIDs remain report-only because native outlines do not store individual edge IDs.
No library or model paths are resolved. Known artwork, mask/paste, 3D models and
rules are reported as omitted. Unsupported copper, transforms, layers, pad shapes,
outline geometry or unknown semantics block the entire conversion.

Save existing native changes and finish or cancel active editing drafts before
**Apply PCB import as unsaved native document**. Apply rereads and reparses the
file and checks the exact preview source SHA256, current project UUID and full
event cursor. Changed source bytes, peer edits, same-project reopening, unsupported
data and unsaved native work reject without replacing the document. Apply also rejects the active native save path and canonical aliases as a source, even after external byte changes.

Successful Apply starts a new unsaved native document with a fresh project UUID
and event stream, clears old history/selection and keeps the external source out
of the active native save path. Choose **Save As…** to write a new native file.
The conversion remains dirty even after editing and undoing back to its initial
state; it cannot be silently discarded as a clean unnamed document. Import is a
lifecycle replacement like Open, rather than an undoable edit. Subsequent native
edits use the ordinary undoable command engine. Source files are never written by
Preview or Apply. The completed conversion report remains labeled as its source
snapshot; later native edits do not update that report.

Agents use the same lifecycle through `preview_pcb_import` and `import_pcb`.
Pass the preview's `path`, `expected_project`, `expected_cursor` and
`source_sha256` (as `expected_source_sha256`) to Apply. Parsing and regular-file IO
run outside the engine lock; final replacement guards run under that lock. Inputs
reject unknown fields. A file is limited to 8 MiB, depth 32, 100000 tokens, 20000
lists/report entries, 4096 bytes per string/path and 2 MiB aggregate token text.
Combined physical objects are limited to 10000, with native pad/outline limits
checked before expensive validation. Linux nonblocking file opening rejects a
regular-file-to-FIFO race without waiting for a writer.

This is a standalone supported-geometry conversion, not schematic/project import,
round-trip compatibility, corpus-wide fidelity or equivalent manufacturing output.
Use native DRC and inspect omitted rules before fabrication. Format dates tested
by this grammar are 20221018, 20240108 and 20241229; this does not imply every file
from those releases is supported. [Public-source behavior spec](specs/pcb-sexpr-import.md)
defines the exact subset and exclusions.

Cardinal pad normalization preserves centers and exact rectangular copper: 90/270
swap width/height; 0/180 preserve dimensions. Reports disclose source/canonical
angles and normalization. Containing footprints still require front zero-angle.
[Cardinal normalization spec](specs/cardinal-pad-import.md) defines the exact bound.
[Verified cardinal fixtures and native evidence](cardinal-pad-import-verification.md)
record exact geometry, real MCP lifecycle checks and the fixture's actual DRC violation.
