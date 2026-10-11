# Native document lifecycle

Public behavior reference: the [KiCad PCB editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)
documents New/Open and native file management. Viamind independently defines the shared-session
and concurrency rules below. No external EDA implementation code is used.

## Shared engine

- `new_project {name?, discard_dirty?, expected_cursor?}` and
  `open_project {path, discard_dirty?, expected_cursor?}` replace the document inside the existing
  engine handle. UI, MCP connections and change subscribers keep sharing that handle.
- Default New name is Untitled; reject blank names and empty paths. Open fully parses and validates
  a native file before replacing anything. Failed reads, validation or guards preserve all state.
- Replacing a dirty document requires explicit `discard_dirty: true` and the current session cursor
  as `expected_cursor`. Obtain it from `get_session_state`. Any supplied stale cursor rejects, even
  for clean documents. Concurrent edits during loading reject rather than being discarded.
- Serialize file saves and replacement operations; release the engine mutex during file reads.
  Reset selection, undo/redo, event log baseline and transaction numbering on successful replacement.
  Start a fresh session journal: old cursors return a resynchronization error and waiting observers
  wake. Existing change subscribers receive a replacement notification and rebuild their views.
- New is clean and unnamed; Open is clean at its absolute path. Return the new session snapshot.
  Replacement is a document lifecycle boundary, outside undoable design transactions.

## Desktop

- File New (Ctrl+N) and Open (Ctrl+O) present a name/path field. Cancel writes and replaces nothing.
- Before New/Open or closing a dirty document, offer Save, Discard, Cancel. A discard applies only
  to the exact cursor shown by the confirmation. A stale confirmation returns an error and requires
  another choice. Save uses the existing shared Save/Save As flow; failed saves and newer unsaved
  edits keep the current document open. Cancel also cancels any pending New/Open/close operation.
- Native window close and File Quit use this same confirmation. Opening or creating a document
  clears view selection caches and fits the new board. Every shared action has a matching MCP tool.
