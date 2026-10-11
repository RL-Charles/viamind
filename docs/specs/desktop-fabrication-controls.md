# Retained desktop fabrication export controls

## Public sources

- [Rust thread Builder::spawn](https://doc.rust-lang.org/std/thread/struct.Builder.html#method.spawn): named thread creation returns an I/O result when the OS cannot create the worker; the unconfigured free function panics on that failure.
- [egui0.36.2 Window::vscroll](https://docs.rs/egui/0.36.2/egui/struct.Window.html#method.vscroll): vertical scrolling keeps a long window accessible in compact viewports.
- [egui0.36.2 Context::request_repaint](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.request_repaint): a worker may request a repaint on completion; scheduled polling handles a retained worker while its form is hidden.
- Existing supported format behavior remains specified in [fabrication-gerber-xnc.md](fabrication-gerber-xnc.md), using public Ucamco/XNC references. This increment changes controls and request sharing, not the format or geometry.

## Behavior

1. Keep the existing background fabrication worker. Use one retained receiver, a fallible named start and a visible recoverable error if startup fails. Repeated starts while pending do nothing. Closing/reopening the window retains the job; poll before the hidden-window return and request repaint on completion. Application exit may interrupt a worker; no resumability claim.
2. Share a typed generic FabricationExportRequest<Cursor> between desktop and MCP. Preserve fields path, mask_expansion_mm (default0) and expected_cursor, schema, defaults and returned path/manifest/exported_cursor. Validate empty path and finite mask expansion0–10mm before snapshot work. Check expected cursor before capture budgets/admission, capture actual cursor/native path together, then retain bounded FabricationSnapshot through publication.
3. Require an explicit new output directory. Existing destination files/directories and the native project remain protected; atomic no-replace publication remains unchanged. Rendering, staging and filesystem work run outside the UI thread and engine lock. No extra worker or unbounded queue. Keep all format counts/text/output/admission limits.
4. Bound editable path and mask text, disable controls while pending, show readable progress/errors, and retain inputs on failures. Show successful path/file count and complete stream UUID plus sequence. When the current cursor differs, explain that the design changed since the exported snapshot. Export changes no shared design/selection/history/dirty state.
5. Vertically scroll the dialog so Export and result/error controls are accessible at640×400. Preserve existing menus and supported outer copper/mask/profile/through-via exports, coordinate conventions, exclusions and generic status. No geometry/model expansion, sourcing/manufacturer presets, vendor acceptance or fabrication certification.

## Verification

Actual File/Board menu and form input, independent native/CLI/MCP file byte parity, exact full cursor, pending/hidden/disconnected/spawn failure lifecycle, compact scroll-to-Export and result/error, invalid mask/path, stale/busy/budget/native/collision errors preserving existing outputs and complete session/status. Full Linux gates, fresh exact native capture/report and fresh combined CI before merge. Retain portable reproduction sources and evidence; binaries stay out of git.
