# Linked component cross-probing

First establish explicit symbol/PCB component links through schematic and PCB synchronization. Links use persisted UUIDs; matching reference text alone does not create a link.

The **Linked component** panel appears at the top of either editor sidebar. Selecting a PCB component highlights its schematic symbol in pink. Selecting a schematic symbol highlights its PCB counterpart in pink. Selection alone keeps the current editor and camera. The counterpart highlight is local and does not add it to shared edit selection.

Use **Show schematic counterpart** or **Show PCB counterpart** to switch editors and center the exact target. The schematic target opens its containing sheet. Hidden PCB layers stay hidden and are explained in the panel. Finish active board edits first; a schematic object draft is retained and must be finished/cancelled before navigating to a different symbol. Missing/unlinked counterparts and unsupported coordinates are explicit. Cross-probing handles at most 100 selected source objects.

A peer edit, unlink, deletion or document replacement invalidates old results. The bounded worker refreshes the mapping; stale results cannot navigate old identities or coordinates. No model, history, dirty state or shared selection changes are made by highlighting or navigation. Board selection retains its existing shared engine semantics, while schematic selection stays local to that editor.

## Agent query

`query_linked_components` exposes the same stable mapping and exact integer nanometre positions over MCP:

```json
{"object_ids":["PART-OR-SYMBOL-UUID"],"offset":0,"limit":100}
```

Omit `object_ids` to page all components. `limit` is 1..100 (default100); at most 100 unique source IDs are accepted. A linked row is returned once even if both its symbol and part IDs are requested. Unknown requested IDs are listed separately. Rows explicitly distinguish linked, unlinked part, unlinked symbol and unresolved symbol states. Navigation flags reject positions outside ±10000 mm without rounding the original integer coordinates.

Restart pagination after `source_cursor` changes. Mapping rejects more than 10000 combined sheets/symbols/parts,4096 bytes per reference,2 MiB aggregate references or ambiguous relevant identities. It does not evaluate electrical connectivity, infer links, propagate selections or cross-probe pins/nets. Camera and highlights are local view preferences; agents can use returned stable identities with existing selection/query tools.

See the [behavior specification](specs/component-cross-probing.md) for ownership, bounds and guarded navigation.
