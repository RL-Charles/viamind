# Explicit SMD aperture verification

T-0120 implements the independently approved [public contract](specs/smd-mask-paste-apertures.md). Rectangular mask/paste openings belong to an existing pad's center, axes and mounted side. No stencil design, percentage hierarchy, islands, via tenting or manufacturing approval is inferred.

## Meaning and compatibility

- An absent outer aperture policy is Legacy. Explicit nullable `mask` and `paste` fields distinguish an exact centered rectangle from intentional omission. Zero/negative dimensions are invalid.
- Exact positive integer nanometres and checked doubled extents retain odd dimensions. The calculator uses checked copper dimension plus twice a signed margin; later copper resizing preserves the absolute authored opening.
- Native/log minimum14 and detached library3 compose with existing features. Raw native/library/custom typed command inputs reject duplicate fields. Initial `serde_json::Value` ownership is not claimed allocation-free and cannot reconstruct duplicate keys already collapsed by a caller.
- Old native/library serialization, seven original fabrication files and default board SVG bytes are tested exactly. Legacy desktop mask remains its inherited +0.05mm border; static mask uses nominal zero and fabrication uses the request expansion. These Legacy previews are explicitly different. Explicit rectangles use their own dimensions everywhere.
- Default scene layers and export requests retain their old behavior. Stored non-null paste with an export that does not request paste rejects before publication. Saved job5 requires explicit policy for every pad and rejects legacy preview parts with unknown pad membership; known empty pad sets are allowed.

## Source and review boundaries

Product interfaces were frozen at `8f5af1d653cade9d3e9e019e80a601964a98611d`, tagged `verification/t0120/consumer-interfaces`; model/native/library/log content is unchanged from foundation `950f6160cd251072a04a0bf47bc25d24313434fc`. Subsequent source `2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed` adds verification cases, the original fixture, inspection host and runtime verifier. Root accepted the interface handoff before separate NPTH integration. No NPTH support is claimed by this ticket.

## Independent geometry and release checks

The new native case compares every explicit corner against a separate doubled-integer oracle for four cardinal rotations on both sides, including odd widths/heights. It asserts unchanged copper and exact Legacy scene restoration. Fabrication tests independently parse emitted openings at the same eight poses; arbitrary milli-degree transforms follow the existing finite computed transform and nearest-nanometre center quantization, not an analytic exactness claim.

Saved job5 inspection independently parses the four retained mask/paste files with shared file/command/primitive caps. It checks declared sides, polarity, common project coordinates, actual rectangle side counts and existing untented-via mask circles. Five semantic corruption cases update declared file lengths where needed, so refusal does not merely prove byte-length checking. Old job checks-performed/not-performed vectors remain exact; the additional checks appear only with observed aperture data. Original geometry authentication, DRC authorization and manufacturing readiness remain outside these checks.

The authored generic/checked companion matrix covers all twelve supported combinations, including optional Legend/calibration/population companions and the checked receipt. The bounded saved map permits the exact maximum17 files while version-specific matrices still reject extras. The opaque validation callback used by the parser matrix is a fixture, not a DRC pass claim.

## Editor and guard checks

Actual 640x400 egui input reveals each field/action and checks effective ancestor clipping before input. Cases exercise exact applied policy, independent omission, negative/equal-margin refusal, stale Apply/Reload, Undo/Redo/Cancel, detached NEW save, Close/Reopen and no implicit component application. Another case retains the focused aperture text's accesskit/egui ID and character cursor through earlier pad removal. Initial preparation failures refused input on clipped rows; the corrected oracle first reveals the outer form and then uses actual wheel input in the nested pad table. Working layout was preserved.

The shared command requires project, full initial transaction cursor, exact Part and existing pad UUID. Typed/raw/batch cases cover count poison before secondary decoding, extent overflow, missing fields, same-UUID reopen, peer edit-plus-undo and complete batch rollback. Library reuse authoritatively replaces policy while retaining matched pad UUIDs/nets and unrelated metadata. NEW publication retains existing staged no-clobber/latest-native guards.

## Retained verification

The original product build at `2cbe6dd6e1bb1fbe9d19d1b2950313d12db902ed` passed fmt, strict workspace/all-target Clippy, workspace tests **1,009 passed / 0 failed / 1 existing ignored**, and workspace build. The full script then exited 1 because a verification helper expected a hidden restricted writer to return a tool error; the application correctly returned a protocol denial. That partial 41-check receipt is retained. The helper alone was corrected, with no admission or product change.

A separate build touched all Rust inputs under the shared mutex immediately before the normal application build. Both retained production and original inspection executables are bit-identical to the full-gate copies. The fresh production SHA256 is `3dbc1f23b408a35fad2ded07252c20430caae931221b944e07ff789d8066d139`. Corrected checks pass: **43 actual stdio, 45 actual desktop two-client HTTP, 118 catalog tools, 18 agent-workflow checks and 13 Python tests**. The HTTP listener is the retained owned fixture on port 7961, not a mock server.

Root independently passed 11 additional two-client checks, including exact creator delta visibility, stale peer rejection, peer refusal to undo creator history, equal/no-op, invalid geometry, creator Undo/Redo and final whole-project/native-path restoration. An earlier independent harness incorrectly attempted peer Undo; its failed receipt and guarded restoration remain indexed separately. Event cursor advancement from those checks is disclosed rather than presented as session immutability.

### Native evidence

The placed editor and board captures use the original product source 2cbe and original inspection binary `ed8a9dd6e0436e90bceac777e60f7cf2a556978787f9bb7858b7330176ba8a1f`. The final compact completed-save capture uses a later inspection-only source; the master records its distinct source and executable hash. It changes no product interfaces.

Two earlier compact preparations successfully wrote their NEW detached libraries, then refused a final visibility assertion before native launch. The heuristic had matched a nearby offscreen TextEdit's clip to the pinned Save button. The inspection-only correction matches the actual painted response rectangle corners within 0.5px and retains full ancestor-clip containment assertions; no product layout was changed. Failed receipts and created files are preserved.

- `placed.png`: 1600x1100 actual production pad-opening editor, four exact dimensions and reachable Apply/Reload/Cancel.
- `board.png`: 1600x1100 actual native two-side explicit mask/paste layers and intentional omission, existing tracks/vias/cutout retained.
- `completed.png`: 640x400 actual detached completed NEW save notice and pinned Save/Close controls. Geometry/field editing is covered separately by actual compact input tests; this completed view does not claim all fields are simultaneously visible.

Each inspection host uses actual egui Harness input to prepare the production application and transfers its state/memory to eframe. Native WGPU Screenshot events retain original P7 RGBA, original P6 RGB and lossless PNGs; every original RGB sample is independently decoded and compared, with original alpha statistics retained. Each host checks complete SessionSnapshot and DocumentStatus, source bytes and effective current control clips, then self-closes. This is not an OS-input or frame-cadence claim. The separately retained tiny compositor production screenshot establishes launch provenance only.

The [master](verification/t0120/master.json) indexes source boundaries, exact bytes/hashes, passing and failed reports and retained executable/raw-channel paths. Run `PYTHONDONTWRITEBYTECODE=1 python3 docs/verification/t0120/verify-artifacts.py --retained` for the complete local audit; omit `--retained` for repository-only checks. Exact-head CI and final independent review remain required before merge.

## Final integration with main

Final scoped source `b3cf88f1c661899bdadade348af5570e448daba0` includes exact main `223d8e770cb626eff4a02110612602e6b1476775` (connected schematic drag). The sole additive UI action conflict retained both drag actions and SetPadApertures. One new drag test Pad literal required `apertures: None`; its initial strict compile refusal is retained. No aperture product behavior changed after the accepted interface handoff.

The corrected combined gate passed fmt, strict workspace/all-target Clippy, **34 focused tests / 0 failed**, workspace build with immediate Rust-input retouch under the mutex, **120-tool exact catalog, 18 workflow checks and 43 additional actual stdio checks**. The retained combined application SHA256 is `b213c09ee79e0d2146b3fa07507e9d3ef3afdec787f0bdd585b19f2ad0161d2e`. Both generated catalog files already matched the merged canonical files byte for byte. Historical full/native/runtime source identities above remain unchanged; no second full gate or duplicate native capture is claimed. Exact-head CI and final independent review remain required.
