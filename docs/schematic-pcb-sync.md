# Linked schematic and PCB synchronization

This workflow updates existing native PCB components and explicit numbered pads. It uses stable symbol UUID links, so renumbering references preserves the association.

## Desktop

Open **Synchronize…** from either editor. Expand **Component links**, choose an existing PCB component and schematic symbol, then use **Link selected components**. Links are explicit; matching reference text does not create an association. **Unlink PCB component** removes the selected association without removing either component. A symbol can link to at most one PCB component. Refresh a stale component list before linking.

Choose a direction and **Prepare synchronization**:

- **Schematic to PCB** copies linked reference/value fields and assigns every explicit pad the exact native schematic graph net of its equally numbered pin.
- **PCB to schematic** copies linked reference/value/footprint assignments. Symbol geometry, pin identities, wires, labels and topology remain unchanged.

Inspect the proposed changes and net names before **Apply synchronization**. One Undo restores the complete update. Preparation runs in one bounded background worker. Document edits or changing direction invalidate the old preview; prepare again. Reopening the same file also invalidates it.

Use the original [linked regulator example](../fixtures/schematic/linked-regulator.viamind.json) to try the workflow. Its existing PCB pad geometry is intentionally simple. Forward preparation proposes three reference changes, three value changes and seven pad-net assignments. The expected groups are VIN (U1.1/C1.1), 3V3 (U1.3/C2.1), and GND (U1.2/C1.2/C2.2).

## Agent tools

`link_schematic_symbol` requires `expected_project`, an exact current `expected_part`, and an explicit tagged `target`: either `{"kind":"symbol","expected":CURRENT_SYMBOL}` or `{"kind":"unlink"}`. Obtain current objects through the query tools. Omission never means unlink.

Call `preview_schematic_pcb_sync` with `{"direction":"forward"}` or `{"direction":"back"}`. It returns proposed changes, unlinked objects, graph net identities and a literal `command`. Pass that exact command to `synchronize_schematic_pcb`; do not reconstruct its guards. Typed engine calls, dynamic commands and command batches enforce the same initial event cursor and whole-project snapshot. MCP connections retain their own undo scope.

Each schematic graph component uses a reserved board net name `sc:<sheet UUID>:<member UUID>`, with canonical label names and scoped aliases returned separately. Identically named local labels on different sheets remain separate; explicit global/power labels join according to the native graph. Connectivity is derived from exact integer contacts, not reference strings or rendered pixels.

## Rejections and limits

Forward updates require an exact, unique pin/pad-number bijection. Parts without explicit pads, missing linked symbols, footprint assignment/geometry mismatches, invalid identities and reference collisions reject the whole update. Unlinked objects remain unchanged. If a changed pad assignment touches either an old or new net name already present in tracks/vias, synchronization rejects and requires explicit routed-net migration. Existing unchanged routed assignments are preserved.

Preparation checks structural/text budgets before copying: at most 10000 total board/schematic objects, pins, pads and outline vertices; 4096 UTF-8 bytes per string; 4 MiB aggregate text; 8 MiB serialized project guard. The exact schematic graph also enforces its 2 MiB text and 1000000 candidate-contact limits. A work-limit failure produces no partial net update. Apply recomputes the plan atomically under the engine lock and can delay editing on large inputs; no worst-case latency or frame-rate guarantee is claimed.

Linked native files require schema6. Files without any links retain their previous minimum schema and omit the optional field. Event logs preserve historical links, including transactions later undone/unlinked, and use log schema6. Deleting a linked symbol keeps an explicit unresolved association; resolve or unlink it before synchronization.

This increment does not create/delete components, replace footprint geometry, migrate routed nets, reverse electrical topology/net names, implement hierarchy/buses, or certify fabrication/electrical completeness.
