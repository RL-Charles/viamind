# Reachable compact detached symbol definitions

## Public behavior sources

Reviewed 2026-10-11. This independent behavior specification uses public documentation only; no other EDA source, library, icon or UI asset is copied.

- [KiCad 9 symbol editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols): a symbol definition has metadata, geometry and individually editable electrical pins. Viamind retains its existing single-unit rectangular native subset.
- [Pinned egui 0.36.2 ScrollArea](https://docs.rs/egui/0.36.2/egui/containers/scroll_area/struct.ScrollArea.html): the inner rectangle clips scroll contents; minimum scrolled height controls how far a required scroll region can shrink; row rendering supports bounded visible ranges. Scrolling to an allocated overscan widget alone does not prove its pixels or actionable rectangle are visible.
- [Pinned egui 0.36.2 Id](https://docs.rs/egui/0.36.2/egui/struct.Id.html): widgets with retained interaction state need identity stable across frames. Existing T102 editor/pin/field UUID identity remains authoritative.

## Observed failure and supported behavior

The existing native symbol-library window has an outer vertical scroll area and an inner virtualized pin area. At a 640×400 logical-point content viewport, preceding reuse/export/metadata content can leave the pin viewport only a few points high. Later pin Y, electrical-type and removal controls can remain clipped even though their accessibility nodes exist. This ticket fixes that functional layout issue.

At 640×400 logical points with the ordinary default UI scale, without resizing the native window, ordinary wheel/scrollbar scrolling and normal keyboard field editing must reach:

- Definition name, default value, optional footprint, width and height.
- Every ordered pin's number, name, exact X/Y millimetre fields, electrical-type popup and Remove control, including later virtualized rows.
- Add pin, NEW file path, Use, Save NEW, Cancel, validation errors and completed-save notice.

All controls need not be visible simultaneously. A complete ordinary pin row can be revealed in the compact viewport; metadata and final actions can occupy separate scroll positions. Long paths/messages may wrap and extend the scrollable content. Existing library load/target/source/export/placement controls must also remain reachable. Native OS scaling beyond the tested logical viewport and smaller windows have no new guarantee.

## Preserved contracts and bounded work

- Keep at most 1024 ordered pins and bounded visible/overscan row rendering. A large list must not instantiate all 1024 full pin editors each UI pass. Tests must report a concrete visible-row bound at the tested viewport.
- Preserve editor UUID, per-pin UUID and constant field/popup identity through scrolling, deletion of an earlier pin and layout changes. A fresh definition still creates fresh identity. Preserve retained text cursor and electrical choice.
- Preserve exact existing integer-Nm parsing, native template bytes/format, shared UiAction/MCP writer and separate guarded placement. No model/schema/catalog/command changes are needed.
- Editing, scrolling and validation remain local; full session, history, shared selection, document status and native bytes remain unchanged. Successful NEW save creates only the explicitly requested detached file under T107's latest native lifecycle/no-clobber guard.
- Existing pending-worker busy guards, retained receiver/Close behavior and draft preservation remain unchanged. Layout must not load, save, place, discard or reset a draft implicitly.

## Acceptance and evidence

Before implementation, independently review this committed spec. Implement in the symbol-definition/library UI lane and focused actual-input tests; preserve additive unrelated UI hooks during later integration.

Actual compact input tests must first reveal each target through real scroll input and verify the full actionable widget rectangle lies inside the effective painted clip, including ancestor scroll clipping, before sending a pointer click. Accessibility existence or an overscan rectangle is insufficient. Verify keyboard text replacement, electrical popup selection, first/later-row editing and deletion, scrolling away/back, exact odd-Nm values, stable retained IDs/cursor, Add/Use/Cancel, error visibility, NEW path and completed save. Invalid or existing destinations must preserve session/status/native and destination bytes. A 1024-pin fixture must exercise later-row access and assert bounded rendered-row count; fixture setup may use supported shared actions/native loading, not direct product-model mutation.

Run Linux fmt, strict workspace/all-target Clippy, workspace tests/build under the shared gate mutex, plus actual shared MCP/native writer checks and exact canonical tool catalog. Retain source/binary/report hashes. Actual self-closing native WGPU inspection at 640×400 must show a complete pin row and a completed NEW-save notice in separate views, with actual local egui preparation, per-pass full session/status invariants, real opaque RGB readback and accurate UI-pass counts. No OS-input or simultaneous-all-fields claim. Existing production/previews remain preserved.

## Dependency and limits

Initial worktree is stacked on frozen T107 evidence head`548a014d4b28b02e814036bf2bcc847736c32cf8`. Transplant only this ticket's commits when its predecessor merges; final combined CI/prospective-tree equality remains required. Multi-unit symbols, new symbol geometry, manufacturing defaults and broader editor redesign remain separate story work.
