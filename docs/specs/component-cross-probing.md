# Stable component cross-probing

## Public behavior reference

The [KiCad 9 Schematic Editor manual, cross-probing from the PCB](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#cross-probing-from-the-pcb) describes counterpart component highlighting and explicit navigation between schematic and board. These public behaviors guide an independently designed native workflow. No implementation code or proprietary assets are used.

## Native supported scope

Use only persisted T-0066 `Part.schematic_symbol` UUID links. References and values are display text and never establish identity. An unlinked component has no counterpart; a deleted symbol leaves an explicit unresolved link. Query results contain stable project, sheet, symbol and part IDs with a source event cursor. No geometry, model schema, electrical graph, net assignment or history changes are introduced.

`query_linked_components` is read-only and bounded before copying. Support at most 10000 combined sheets, symbols and board parts, 4096 bytes per returned reference and 2 MiB aggregate reference text. Reject nil/duplicate/key-mismatched relevant identities and multiply linked symbols with actionable errors. Missing targets remain explicit unresolved results. Return deterministic pages (offset, limit 1..100, default100), source cursor, total count and next offset. Optional object IDs (at most100, no duplicates) restrict the result to explicitly requested symbols/parts; identify unknown IDs separately. Empty IDs request the full ordered mapping. Restart pages after any cursor change. Positions are integer nanometres; extreme unsupported board navigation coordinates are reported rather than collapsed through floating point.

## Desktop selection and navigation

Preserve existing board selection in the shared engine and existing schematic selection as local editor state. Derive counterpart highlights locally without propagating SetSelection, changing history/dirty state or selecting the counterpart as an edit target. Selecting a linked board part highlights its schematic symbol; selecting a linked schematic symbol highlights its board part. Selection alone never switches editor mode or camera. Clearing the source selection clears its derived highlight. Selecting unsupported objects produces an explicit no-component message. For board multi-selection, handle at most100 source objects, with explicit limit guidance rather than a silent partial set.

Provide an accessible component cross-probing panel and explicit Show counterpart buttons. Navigation validates the current project, complete stream/cursor, source identity and persisted link again. It switches through normal editor mode/gesture guards and centers the exact target. Schematic navigation chooses its containing sheet. PCB navigation preserves layer visibility and explains if the target is on hidden layers; a highlight never silently reveals layers. Local view navigation and highlight preferences are view-only actions; MCP exposes the same stable mapping/positions through the query.

Cached query results must match current document, cursor and source selection before highlighting or enabling navigation. Document replacement, same-ID reopen, link removal, object deletion and reference renumbering receive explicit handling; stale rows never navigate old coordinates. A bounded background request uses one retained receiver through replacement; older completion cannot overwrite a newer context. No per-frame whole-project clones, unbounded queued work or UI-thread joins.

## Verification

Use the original linked regulator fixture. Exercise both selection directions, exact immutable navigation, renamed references, unlinked/unresolved IDs, deletion, selection clearing, multi-selection limits, stale edits and same-ID reopen. Verify deterministic pagination and unknown IDs, rejected bounds/invalid identities, unchanged model/history/selection/dirty state and two real MCP clients. Actual egui input tests and Linux fmt/clippy/test/build must pass. Inspect native readable counterparts and hidden-layer guidance. Net/pin cross-probing, inferred links, hierarchy, simultaneous split editors and propagated counterpart selection are excluded.

## Stacked development base

This ticket begins on T-0066 feature head `6ff809c4aa605a93341f5482fa740cdfb7be54f3`. After T-0066 merges, transplant only T-0070 commits onto merged main and run fresh combined CI. T-0066 remains independently owned and reviewed.

T-0070 commits were transplanted onto merged main `30e7531`, preserving ERC, synchronization and routing. Explicit navigation also disables startup resize auto-fit; actual tests cover first-ever schematic navigation.
