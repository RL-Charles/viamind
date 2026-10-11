# Desktop generic assembly CSV export

## Public sources

- [RFC4180 section2](https://www.rfc-editor.org/rfc/rfc4180): CSV records, quoting and escaped quotes. Existing generic Viamind BOM grouping/origin placement behavior remains specified in assembly-csv.md.
- [Rust thread Builder::spawn](https://doc.rust-lang.org/std/thread/struct.Builder.html#method.spawn): fallible named worker creation. Report failure and retain a usable form.
- [egui0.36.2 Window::vscroll](https://docs.rs/egui/0.36.2/egui/struct.Window.html#method.vscroll) and [Context::request_repaint](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.request_repaint): scrollable compact controls and background completion repaint.

## Behavior

1. File → Export assembly CSV opens a local dialog selecting BOM or placement and an explicitly entered .csv output path. Default BOM and overwrite=false. No automatic destination or manufacturer preset. Describe BOM exact value/footprint grouping and placement component-origin/global-mm/Y-down/CCW-top-view convention.
2. Default publication requires a NEW file. A clearly labeled checkbox explicitly permits replacing this CSV output. The active native project remains protected even with overwrite. Stage render/write/fsync outside engine locks, then serialize final path checks and atomic publication with complete native open/new/save lifecycles. Protect both the captured source path and the current active path, including a save-as still writing when export finishes staging. Native/collision/budget errors preserve all existing bytes and keep editable form inputs for correction.
3. Share one typed generic AssemblyExportRequest<Cursor> and one schema provider between the desktop action catalog and existing export_assembly_csv MCP tool. Existing tool fields, defaults and results remain compatible. Capture with bounded AssemblySnapshot, check expected_cursor first and capture actual cursor/native path in the same engine lock.
4. A single retained worker performs capture/render/publication outside the UI thread. Repeated starts while pending do nothing; closing/reopening the dialog retains the job. Disable request controls while pending. Handle spawn/channel/write failures visibly; poll without blocking and request repaint on completion. Application exit may interrupt a worker; do not claim resumability.
5. Results show actual path, kind, component count and complete source cursor. Show a newer-edit notice when the current cursor differs. Export changes no design, shared selection, history or dirty status. Existing rendering/fabrication menus remain available.
6. Use a vertically scrollable window and accessible field labels, bounded path input, visible progress/status and generic format exclusions. Existing100000-component/4096-byte-string/2MiB-text/32MiB-output and shared two-snapshot admission limits remain. No supplier/MPN/population/centroid inference or fabrication acceptance claim.

## Verification

Actual File-menu/form input for both CSV kinds with independent parsing and production/library/live-MCP byte equality. Check empty/extension errors, NEW collision preservation, explicitly enabled overwrite, active-native protection, stale guard/busy/budget errors, unchanged session/status, pending/hidden/disconnected worker lifetime and newer edits. At640×400, scroll to Export and result and use actual pointer input. All Linux workspace gates and exact native production/MCP evidence before submission; retain portable artifacts.
