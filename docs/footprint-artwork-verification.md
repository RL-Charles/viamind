# Explicit footprint artwork and Legend verification

T-0119 adds mounted-side straight silkscreen strokes and one rectangular courtyard to native components and detached footprints. An omitted artwork member preserves the existing synthetic body; an explicit empty artwork object suppresses that body. All stored coordinates/stroke widths remain integer nanometres.

## Authoring and agent parity

Open a component's footprint artwork editor from its properties, or author artwork in the detached native footprint definition editor. Apply artwork, Reload artwork and Cancel remain outside the bounded scrolling form. Local rows use stable presentation identities; these IDs are never persisted. Detached Save NEW, Preview and Close preserve the retained worker lifecycle.

Agents call `set_footprint_artwork` with exact `expected` Part, `expected_project`, full initial `expected_cursor`, and explicit nullable `artwork`. A changed edit creates one Part delta and one undoable event. Stale cursor/original/project refuses without a partial edit. An exact no-op emits no event. `artwork:null` restores legacy drawing. Existing pad IDs/nets and all unrelated component fields remain intact.

Library version 1 omits artwork. Version 2 requires non-null artwork, including explicit empty; raw duplicate and unknown fields refuse. Guarded library apply/export retains art and matching pad IDs/nets. Native/event feature 13 max-composes with package 12, via 10 and assembly 9. Clearing a feature permits the exact remaining minimum version.

## Outputs and boundaries

Board SVG/PNG support opt-in top/bottom silkscreen and courtyard layers. Existing default layers and nonempty-art default SVG bytes stay unchanged. Actual desktop pose uses the shared mirror/CCW rotation/translation; cardinal cases are exact. Arbitrary-angle output rounds the finite computed transform to nearest Nm, at most 0.5 Nm relative to that computed value, rather than claiming analytic exactness.

`export_fabrication` defaults `include_silkscreen:false`. True adds paired positive Gerber Legend Top/Bot files containing only stored straight strokes. Output-job version 4 requires true and composes the existing optional calibrated/populated companion relationships. Older job bytes/matrices remain unchanged. The independent saved-release reader verifies the declared opt-in matrix and retained art summary.

There are no footprint text/arcs, silk clipping, pad-to-silk/courtyard clearance checks, courtyard manufacturing output, general polygons or manufacturing certification in this increment. Existing solder-mask fabrication remains its scoped legacy behavior. Stored artwork does not participate in electrical continuity. The ordinary assembly-free clone retains it; a separately named private electrical-analysis shell omits unused artwork before copying. Exact guards, committed deltas, branches and native saves retain it.

Bounds are 256 silkscreen rows per component plus one optional courtyard, checked positive strokes and doubled stroked extents in the ±10000mm local range. Existing consumer structural/text/serialized budgets charge borrowed art before copies. Detached libraries retain the existing 1 MiB input bound. No new global native-file size bound is claimed.

## Linux and production evidence

Original verified source: `004960d9729a1fe9a19da6ed5fb35b39c9f581a6`, retained by `verification/t0119/native-source`. Formatting, strict workspace/all-target Clippy, workspace tests: 923 passed / 0 failed / 1 ignored including doctests, workspace build and native inspection build all passed. The first full run is retained as failed: it exposed five legacy-layer/default-scene test failures, corrected by making courtyard layers appear only for stored courtyards, including live Undo/redo coverage.

Original production SHA256: `e2fa0a09e3951713a74b34e836f7e12e18888eca2aa4f4aee124c6fc7bfffa0e`. The exact live process 2960857 on owned port 7954 matched it. Two independently initialized MCP clients passed 34 checks: guarded edit/event/no-op/stale lifecycle, creator Undo/redo, peer invalidation, both-side geometry/pad identity, default SVG/fabrication byte parity, strict lib2 round trip, native 13 save/reopen, explicit Legend/job 4 release/reader, read-only stdio and no-display CLI parity. This is a bounded fixture, not a throughput benchmark. Observed HTTP latencies: top edit 1.98 ms, bottom edit 1.95 ms, Legend export 9.45 ms, job 4 release 12.80 ms.

Original inspection SHA256: `ab29326417dee8620f067786ebeda02f615ecc856b3959c89ba5a7cc06efe9c3`. Wide 1600×1000, compact 640×400 and board 1600×1000 logical hosts self-closed after 12 UI callbacks each. Original framebuffer dimensions 2667×1667 and1067×667 reflect native scaling. Original RGBA is retained in P7; P6 and review PNG preserve every identical RGB sample while omitting alpha. Editor alpha 247..255 and board 242..255 are explicitly retained; opacity/compositor capture is not inferred.

The final inspection host additionally transfers prepared egui scroll memory and safely checks the current native silk-width field and persistent footer response rectangles. Both original and final capture provenance are retained; original courtyard-at-top images are not reattributed to the final host. Actual egui preparation/logical focus is disclosed; there is no OS-input or frame-cadence claim. Cua exact-PID discovery established the production window; AT-SPI degraded to metadata only, so checked state comes from actual MCP and inspection invariants/native WGPU readback.

## Final integration

Own-only transplant onto immutable T122 `1e113aa17e158510b844a4a23d1e8cace4ae1b6f` preserves merged T117 agent modes and T122 schematic label layout. The 19 original feature commits have only additive constructor/CLI/README context differences; the follow-up changes only the inspection host. Combined source/gates, exact generated tool union/workflow, final mode admission and refreshed native images are indexed in the master report. Historical full/source/runtime evidence remains tied to its actual source and binaries. Fresh exact-head CI and independent review are required before parent merge; human Done remains separate.

Final product source `d09dfe016f2852191a97ee9d670cadd885a49d9d` passed 34 scoped Rust tests, formatting, strict workspace/all-target lint, workspace build, 114-tool exact catalog, 18 workflow checks and 23 Python tests. Its retained native production binary `1798841869bd1e2b1b25c82b0ae88815942e67505b52f3c05b045d83ddeeba1d` on port 7956 independently passed the same 34 production checks, while separate initialized stdio processes passed 12 Live/Approval/Read only writer-admission checks. Inspection-only final source `9f53d65ddf5bff11c96a7fdd8cba2e19a451789f` uses outer wheel scrolling and painted ancestor-clip proof; earlier field-clipped captures are explicitly retained as excluded visibility evidence.

See [the indexed master](verification/t0119/master.json), [production checks](verification/t0119/production-combined.json), [mode checks](verification/t0119/combined-modes.json), [independent audit](verification/t0119/independent-original-audit.json) and final [wide](verification/t0119/final-artwork-wide.png), [compact](verification/t0119/final-artwork-compact.png), [board](verification/t0119/final-board-wide.png) images. Rehash repository and retained artifacts with `python docs/verification/t0119/verify-artifacts.py --retained`. The portable production probe requires a fresh owned native application launched from the indexed initial fixture; it intentionally mutates only that scratch project.
