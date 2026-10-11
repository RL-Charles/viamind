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

## Verification required

- Actual 640×400 egui input starts with controls below the fold, reveals them using
  supported scroll input, and requires each target response rectangle to be within
  its effective inherited clip and application viewport before clicking. Overscan or
  scroll_to_node alone is not an oracle. Exercise sheet controls, all four Add types,
  Fit, Draw/Cancel wire and annotation/library/ERC entry points with existing guards.
- Create/select a local symbol draft; reach every coordinate/pin/electrical field and
  Apply/Reload/Cancel. Type exact odd-Nm values, scroll away/back and verify draft,
  focus/cursor/selection and popup identity. Include row removal where identity changes.
  Actually Apply once, verify one expected delta and shared Undo/redo; Cancel leaves
  full session/status unchanged. Peer sheet edits and same-ID document reopen retain
  existing stale rejection/cancellation behavior.
- A large bounded object/pin fixture verifies the last supported row/action is reachable
  without unbounded widget construction when virtualization is used; over-budget
  snapshots show actionable admission errors. No claimed throughput from a tiny fixture.
- Wide and compact viewport tests verify Fit/pick parity, local wheel routing and
  complete model/session/status/selection preservation for observation/scrolling.
- Real Linux eframe WGPU inspection captures at640×400 logical size show a revealed
  supported editing action and selected-object Apply/Cancel, with exact framebuffer
  provenance, original RGBA/RGB retention, full session/status invariants and self-close.
  Inspection event injection is disclosed; no OS-input claim without actual native input.
- Linux fmt, strict workspace all-target Clippy, workspace tests/build; actual generated
  catalog/workflow remains unchanged. Retain failed logs and exact source/binary/capture
  hashes. Independent source/native review and exact-head CI precede parent merge.
  Human Done remains pending; this agent does not merge its own PR.
