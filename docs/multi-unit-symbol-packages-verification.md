# Multi-unit symbol package verification

T-0114 implements the independently approved [public behavior spec](specs/multi-unit-symbol-packages.md). The [workflow guide](multi-unit-symbol-packages.md) describes the supported definition, placement and deletion operations. This is an original authored fixture; no external symbol or library assets were used.

## Exact source and Linux gates

Production and all three native inspection modes were built from `fb9be124977b11100b0549e53555fdad3ca95e3c`, based on merged T113/main `deb0141`. The immutable master records source references, retained executable hashes, original framebuffers, process identities and every durable artifact. The full gate held the shared workspace mutex through doctests and executable copies, refreshed local source mtimes, disabled incremental compilation, and used the canonical target path.

- Formatting, strict workspace all-target Clippy, workspace build and native inspection build passed.
- **874 workspace tests passed, zero failed, one existing test ignored**, including doctests.
- Thirteen Python MCP tooling tests, discovery of **109 actual full-application tools**, and eighteen isolated agent workflow checks passed.
- The committed full MCP catalog was regenerated from that retained executable and checked byte-for-byte against a fresh discovery.
- Three actual compact input regressions passed: loaded power-pin editing; authoring all five units and fourteen pins from an empty editor, strict NEW-file equality, atomic placement and undo; and the Libraries menu entry.

Earlier compile/lint failures and their source identities remain documented in the ticket and retained logs. They are not represented as passing gates. The compact focused run passed its three tests before failing on an unused helper in the inspection example; the corrected full gate subsequently passed all targets.

## Actual executable and shared MCP behavior

The retained production executable SHA-256 is `98663744f73f41b3efba8e8d01216a1286d63c44926bb2fc7828648fbf8f83d2`. It was launched with Cua as PID 2574825 on owned port 7944. The reproducible [native verifier](../scripts/verify-symbol-packages-native.py) passed **38 stdio checks and 38 actual two-client desktop HTTP checks**. A separate reviewer independently reran all 38 stdio checks; its unchanged report is indexed.

The checks cover detached load and NEW save, count/string/identity admission, active-native destination protection, exact complete five-unit/fourteen-pin placement with fresh group/symbol/pin UUIDs, independent mirrored/cardinal coordinate expectations, explicit graph pins, immutable ERC, stale full-cursor rejection, selective undo/redo, complete deletion/undo, and feature-11 native save/reopen. Existing source, protected native bytes, SessionSnapshot and document status are compared where the operation promises preservation.

## Actual native rendering

The retained inspection executable SHA-256 is `e41e6be92731e15baf7423b0bf8a78ffa046fb52edce4b65f621c2ec5085e09c`. The [inspection source](../crates/ui-egui/examples/package_inspection.rs) authors the same definition through actual egui widgets, then transfers the production `ViamindApp` and UI memory to a native WGPU window. Preparation uses injected egui events; **no OS input is claimed**. Each UI pass checks the complete prepared session and document status. Every host closes after the invariant-checked Screenshot event and synchronized framebuffer writes.

- [Compact power pin](verification/t0114/unit.png): the full second power pin row is reachable inside the effective ancestor clip; number 14, V-, exact X `1.000003`, Y `-2.000007`, PowerInput and Remove are visible. The local rectangular body/origin and explicit two power endpoints are shown.
- [Compact completed save](verification/t0114/completed.png): wrapped Created path, five-unit/fourteen-pin count and project/session preservation notice are visible. The pose table is virtualized; this view shows its first rows, not every pose at once.
- [Placed complete package](verification/t0114/placed.png): the same explicit A–D functional units and separate P power unit are placed together and rendered with qualified shared reference labels. Fourteen actual pins are represented; shared reference does not create hidden electrical joins. Existing center-label/pin-text crowding remains visible at fit-all zoom; this capture does not establish that every overlapping text label is legible. T122 tracks a separate renderer/readability correction, preserving geometry and SVG/native parity.

Compact viewports are approximately 640×400 logical points; their physical framebuffer is 1067×667 due to the actual native scale. The master records the exact placed viewport and physical dimensions. Original Color32 RGBA channels are retained as P7, and identical RGB samples as P6. PNGs preserve every RGB byte; alpha is omitted for review, and **opaque readback is not claimed**. The verifier checks artifact lengths/hashes, PNG CRCs and every decoded RGB sample against the original sample digest.

## Supported boundaries

A package is complete on one sheet: all unit and package pin identities are explicit and globally unique, shared properties have one owner, and all-member place/delete is one Sheet delta. Generic member edits preserve group semantics and permit pose changes only. Known counts and strings are admitted before secondary typed allocations and clones, including native/event-log, graph/ERC, renderer and branch sandbox copy paths.

There are no hidden or common power pins, inferred units, cross-sheet package membership, arbitrary body art, stacked pins or imported symbol equivalence claims. At this base, flat annotation, selected-unit PCB linkage/aggregation and selected-unit template export fail explicitly. Package-aware annotation and one-package/one-physical-PCB aggregation are separate T115/T116 increments. Wires survive complete package deletion and are not automatically rerouted. This verification does not claim full application completion, complete ERC or manufacturing fidelity.

## Reproduction

Run the four Linux gates and the retained full-gate script using the shared mutex. Run the native verifier with an isolated binary or two-client owned HTTP URL. Regenerate/check `docs/generated` with `scripts/generate-mcp-reference.py`. Launch the inspection example in `unit`, `completed` and `placed` modes with NEW capture/output paths. Use the retained launcher through Cua for native process provenance. Finally run `python3 docs/verification/t0114/verify-artifacts.py` for the committed evidence.
