# Native project saving

Public behavior reference: the [KiCad PCB editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)
describes editor file management and configurable shortcuts. Viamind's file envelope is independently
defined in `docs/decisions/0001-file-format.md`. Implementation reference:
[`tempfile::NamedTempFile`](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html),
including atomic persistence and no-clobber persistence. No external EDA implementation code is used.

## Shared contract

- The engine tracks the active native path and the exact last saved project. New documents start
  clean with no path. A loaded document starts clean at its absolute path.
- `get_document_status {}` returns `path` (or null) and `dirty`. Document edits change dirty state;
  selection, queries and saving do not enter document undo history. Undoing back to the saved
  project marks it clean again.
- `save_project {path?, overwrite?}` captures an atomic document snapshot and writes the existing
  versioned native JSON envelope. Without path, save the active file, replacing it. If there is
  no active path, return an actionable error. With path, default `overwrite` is false: existing
  destinations are preserved unless replacement is explicitly requested. Unknown fields reject.
- Use a unique temporary file in the destination directory, write and sync it, then atomically
  persist it. No-clobber mode must remain safe if another process creates the destination during
  the save. Failure cleans up the temporary and preserves both prior document status and file.
- Save calls on one engine are serialized, while document edits remain available during file I/O.
  The saved baseline is the captured project, so edits arriving during saving remain dirty.
  Success returns absolute `path`, current `dirty`, and the snapshot's `saved_cursor`.
  Saving wakes UI listeners without emitting a fake design transaction or clearing redo history.

## Desktop

File → Save (Ctrl+S) uses the active path, or opens Save As when no path exists.
File → Save As (Ctrl+Shift+S) presents a path field and explicit replacement option.
Paths may contain spaces; missing parents produce an inline error. Cancel writes nothing.
Successful Save As updates the shared active path. The window/status show the filename and an
unsaved marker, including changes made by agents. Both entry points use the UI action catalog
and the same engine operation as MCP. `--open` initializes the active path.

New/Open replacement and close confirmation follow in the document lifecycle ticket; this
increment adds saving to the existing desktop and headless sessions.
