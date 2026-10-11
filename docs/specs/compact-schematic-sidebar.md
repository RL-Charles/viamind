# Reachable compact schematic sidebar

Ticket: T-0124 · Story: S-041 · Independent implementation from public behavior.

## Sources and observed problem

- [egui 0.36.2 ScrollArea](https://docs.rs/egui/0.36.2/egui/containers/scroll_area/struct.ScrollArea.html):
  the inner rectangle clips contents; bounded height, minimum scrolled size, persistent
  identity and reveal-to-rectangle support are public layout primitives.
- [egui 0.36.2 Id](https://docs.rs/egui/0.36.2/egui/struct.Id.html): persistent widget
  state needs identity independent of changing layout or neighboring rows.
- [egui 0.36.2 Ui](https://docs.rs/egui/0.36.2/egui/struct.Ui.html): inherited clip
  rectangles and wrapped horizontal layouts describe supported containment/input.

The merged T122 native 640×400 logical views show the inherited schematic sidebar
extending below the viewport: Add symbol, Add wire, Fit schematic, Draw wires and
Annotate symbols are outside the visible portion. The production sidebar already
contains an outer ScrollArea. This ticket establishes actual reachability, bounds and
state preservation; adding another unconstrained scroll area alone is insufficient.
No external EDA code, assets or library data are used.

## Behavior contract

1. At a 640×400 logical application viewport, the schematic sidebar has a finite
   content area below the menu and above status. Its vertical scroll region has a
   stable explicit ID distinct from PCB layers, an available-height maximum and a
   minimum scrolled height that cannot force it through its ancestors. A visible
   scrollbar or equivalent existing wheel behavior reveals content below the fold.
   Sidebar width remains bounded by its allocated panel; controls wrap or use bounded
   widths instead of forcing a horizontal overflow into the canvas. No simultaneous
   visibility of all schematic controls is promised.
2. Through ordinary scrolling and existing collapse controls, a user can reach the
   sheet selector/name/Create/Rename/Delete/Reload controls; package/symbol libraries
   and ERC entry points; Add symbol/wire/junction/label; Fit schematic; existing
   object list and selected-object editor; Draw/Finish/Cancel wire controls when
   applicable; and Annotate symbols. Action rows wrap, including selected-object
   Apply/Reload/Cancel, so every enabled control has an actual clickable rectangle
   within the effective ancestor clip. Errors and stale-draft notices remain reachable.
3. Existing selected-sheet snapshot admission remains authoritative: at most10000
   objects,10000 pins and2MiB text. No whole-project clone or new unbounded search is
   introduced for layout. Object lists use bounded visible rows if changed; editable
   symbol pin rows retain the existing maximum1024. A row may be offscreen, but its
   metadata being present in the accessibility tree is not evidence of reachability.
   This ticket makes no frame-latency or unlimited-sheet claim.
4. Scroll and collapse state use explicit stable identities; object rows use UUIDs.
   Any changed editable field identity is derived from a local draft instance, stable
   local pin-row identity and a constant field key, including electrical-type popup.
   Removing another row or scrolling does not transfer text cursor/selection/popup
   ownership to a different field. A newly created/reloaded draft may reset its own
   field state. Local IDs are presentation-only and never enter serialized designs.
5. Scrolling, revealing, collapsing, resizing, Fit and switching view modes are local
   presentation actions. They preserve the complete SessionSnapshot, DocumentStatus,
   board shared selection, native source bytes and event cursor/history. Scrolling
   preserves draft text, selected schematic object, retained workers and editing focus
   unless the user explicitly focuses another control. View changes retain existing
   cancellation/document-replacement rules; this ticket does not make an old draft
   valid after a sheet/project/cursor mismatch.
6. Existing actions use their existing UiAction/engine/MCP command paths and guard
   semantics. Apply creates the same atomic sheet delta; Undo/redo behavior, identity,
   electrical geometry and dirty-state changes are unchanged. Cancel stays local.
   No new persisted command or agent tool is needed for local scrolling and wrapping.
   Agent mode restrictions continue to affect agents while human editing remains live.
7. Polling/synchronization of pending wire/library/annotation state remains independent
   of whether its sidebar controls are scrolled into view. A completed worker cannot
   implicitly apply a draft, steal focus or replace newer source context. Retained
   receivers and existing stale-context tests remain intact.
8. The canvas uses its actual remaining viewport. Wide layout preserves its existing
   default panel width/control order and normal camera behavior; compact layout may
   expose wrapped rows. Existing responsive Fit includes shared label envelopes and
   body/pin picking uses exact geometry. Wheel events over sidebar scroll contents;
   events over canvas keep existing zoom behavior. No label-plan/SVG/native geometry,
   picking tolerance or model/file schema changes are part of this ticket.

## Verification required (baseline-informed amendment)

The baseline proves all inspected action controls already reachable through the
existing outer scroll area. T124 changes only draft/pin/field identity. This amendment
keeps the behavior contract above and makes the acceptance evidence precise; it does
not claim a complete new compact-layout or virtualization implementation.

- At640×400, reveal all14 inspected sidebar entry buttons and Apply/Reload/Cancel,
  requiring complete response containment in the actual painted ancestor clip before
  each action. Actual wheel events reveal the footer without changing the canvas
  camera or full SessionSnapshot/DocumentStatus.
- For text inputs exercised by this ticket, reveal and require effective painted clip
  containment before focus/type. Exercise every symbol metadata/body/coordinate/pin
  text field, both wire endpoints and label name, preserving exact local text; Cancel
  leaves session/status unchanged. Exercise one guarded exact Apply and keyboard
  Undo/redo through the existing shared command.
- Remove an earlier pin after typing into a later pin. Assert retained text-edit ID,
  exact text and caret selection. Exercise the later pin's electrical popup before
  removal and after reindexing; assert identical popup-owner widget ID and correct
  row values. Add a new row and prove it does not reuse the removed/retained owner IDs
  or transfer electrical selections. Explicitly clicking another control may close
  the popup; no promise to keep an open menu through that outside click is made.
- Map inherited tests for sheet creation/name/selection/edit/delete/reload/stale and
  same-ID reopen; wire Finish/cancel/pan/zoom/stale guards; and wide/compact Fit/pick
  parity. Preserve their original viewport/scope in the coverage report. Their
  functional success is not presented as new640×400 effective-clip evidence for every
  inherited field/control. Sheet-selector interaction and max1024-pin/last-object
  stress are not new T124 claims because their layout/virtualization is unchanged.
  Any future change to those controls or virtualization needs its own actual bounds
  and last-row admission checks.
- Real Linux eframe WGPU640×400 captures show revealed entry and selected-object
  action controls with exact framebuffer provenance, retained RGBA/RGB, every-pass
  session/status invariants and self-close. Disclose preparation input; no OS-input
  claim without actual native input. Existing compact canvas remains narrow.
- Linux fmt, strict workspace all-target Clippy, workspace tests/build and actual
  complete catalog/workflow checks pass. Retain failures and exact source/binary/
  capture hashes. Test-only evidence strengthening after the complete gate uses
  focused tests/lint and does not relabel the historical native/product source.
  Independent review and exact-head CI precede parent merge; human Done stays pending.
