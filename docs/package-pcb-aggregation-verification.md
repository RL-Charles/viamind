# Complete package to one PCB component verification

T116 maps an explicitly authored complete package UUID to one physical PCB component. The original fixture contains four functional units and one explicit power unit with fourteen distinct pin numbers. Roles do not imply electrical connections.

## Supported behavior

Shared preview/create/link operations require full source cursors and complete expected project guards. Creation uses an explicit native rectangular SMD footprint, integer nanometre origin, side and cardinal rotation. Every package pin must match exactly one pad number. Forward synchronization uses actual graph pin UUID nets. Back synchronization updates shared reference, value and footprint fields on every member atomically. Routed-net reassignment refuses. Missing package UUIDs remain explicit unresolved associations until cleared.

The optional `Part.schematic_package` scalar is absent by default and mutually exclusive with `schematic_symbol`. Its presence requires native/event version12, composed by maximum with existing features. Both before and after event snapshots require12. The legacy single-symbol canonical fixture was saved by an actual retained prefeature executable; native serialization compares its bytes directly. Borrowed branch serialization has exact native byte oracles for absent, single-symbol and complete-package owners.

No package polarity, centroid, vendor zero, implicit role connection, physical copper continuity, placement clearance acceptance or manufacturing certification is inferred. Run scoped DRC after placement.

## Source and Linux gates

The original full gate used source `4517ac040bc74bc60987704211eb404761fc5b0e`: **828 workspace tests passed, zero failed, one ignored**, plus four focused parity tests, format, strict workspace all-target Clippy, workspace build and native inspection example build. Retained original binaries were not used as final native evidence.

The final combined product source is `37f1a654ffbb0b253393c12fb6a26bd671ccf9cd`, stacked on the literal reviewed T115 boundary `61e73f220e5f92259ac2d43b48ba096da70b3e89`. Its scoped integration gate passed **78 Rust tests**, format, strict workspace all-target Clippy, workspace and inspection builds, actual **112-tool** canonical generation and drift comparison, **18** isolated agent workflow checks and **13** Python tooling tests. No repeated full-suite claim is made for this final combined gate.

The integration includes the actual T115 annotation command: ResetAll/start17 changes all five unit references while preserving the entire PCB Part. Forward synchronization then updates exactly that one Part reference, preserving geometry, pad IDs and nets; each origin-owned undo boundary is checked. The manual borrowed Part serializer includes the package scalar in native field order before any sandbox clone.

## Actual production MCP proof

[Production report](verification/t0116/production-report.json) records **37 passed checks** from two initialized actual HTTP MCP clients on port7946 plus an independently initialized read-only stdio process. The retained production executable is `production-final-combined-verified`, SHA256 `e90c832db066484052fe632b25378471683a409034df33fdcb12bc0bf5947a92`, built from the final combined product source above.

The proof covers immutable complete previews, invalid footprints, stale equal-design cursors, one-Part creation, every member/package/owner terminal query, independent signed integer bottom90 geometry, origin-owned undo/redo, all-member back updates, explicit nullable unlink, feature12 save/reopen and read-only discoverability. It checks its owned scratch project identity before replacing that project. Other applications and projects are preserved.

## Native desktop proof

The inspection-only example hosts the production `ViamindApp` with a fixed1600×1000 native WGPU viewport. Local egui pointer/text events choose the complete package, load the authored footprint, select bottom and90degrees, prepare, scroll and create. Preview capture requires visible exact source/pose labels and unchanged full session. Creation produces one linked physical component with fourteen pads, unchanged schematic identities/geometry and shared selection, and one design transaction. Captures are NEW raw P7 RGBA and matching P6 RGB files, fsynced before the host requests Close. The production unsaved-changes guard initially retained the changed preview; afterward the inspection wrapper reached its1500-frame bound and panicked. Its completed frame60 captures and fsynced invariant report are verified separately from this non-clean post-capture exit. No self-close or clean helper-exit claim is made. The unchanged actual production process remains running. PNG conversion decodes every original RGBA sample and compares it with the P7, then independently checks RGB against the P6. Alpha min/max/nonopaque counts are recorded; no alpha flattening or opacity assumption is used.

The first helper stopped at its1500-frame bound because its readiness condition required a Create button below the scrolling dialog. Its log is retained. The corrected helper captures the visible preview first, then uses bounded local wheel events to reveal the clipped Create control. This is inspection-host behavior; product source and production executable remain unchanged. The helper-only refresh has separate source, executable and build provenance in the master index. Its first compile stopped because the wheel event lacked its required phase; new channel closures then failed strict lint. The next native attempt captured the preview and created the complete owner, but asserted before its clipped completion footer became visible. The final helper waits for that footer after bounded local scroll. Separate logs, sources and executable identities preserve these failed inspection attempts. The next helper passed preview/create with the schematic visible in the background. The final helper selects PCB presentation through the existing local presentation API after the unchanged preview; creation still uses actual local widget input. This presentation-only step is explicitly recorded, and the corrected scoped build is separate.

Cua recovered exact window metadata but explicitly refused screenshot identity (`surface_identity_unproven`) and reported unavailable AT-SPI. This report is retained as discovery evidence. Native pixels therefore come from the application's WGPU Screenshot event, not a Cua capture. Local injected events are not OS input, and no desktop configuration or cadence claim is involved.

## Reproduction

Build the current workspace and the `t0116_inspection` example. Start an owned production instance with `VIAMIND_MCP_PORT=7946` and `--open docs/verification/t0116/initial.viamind.json`. Set `PYTHONDONTWRITEBYTECODE=1` and use NEW output paths. Run:

```sh
python3 docs/verification/t0116/production-probe.py \
  --binary /absolute/verified/viamind --repo "$PWD" \
  --native "$PWD/docs/verification/t0116/initial.viamind.json" \
  --endpoint http://127.0.0.1:7946/mcp --output /absolute/NEW-proof

/absolute/verified/t0116_inspection \
  "$PWD/docs/verification/t0116/placed.viamind.json" \
  "$PWD/fixtures/footprints/original-smd14.viamind-footprint.json" \
  /absolute/NEW-captures
python3 docs/verification/t0116/convert-capture.py /absolute/NEW-captures
```

The placed native fixture was produced by an initialized production stdio MCP session using actual `place_symbol_package` and `save_project`; its source script and report are retained. The master index records source identities, reports, fixture hashes, original/final/helper build logs, executable identities and raw-channel verification. No executable is committed.
