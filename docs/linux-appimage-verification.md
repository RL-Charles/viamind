# Linux AppImage verification

T-0093, PR [91](https://github.com/RL-Charles/viamind/pull/91).
The public specification was independently reviewed before implementation.
See [build/run instructions](linux-appimage.md),
[the specification](specs/linux-appimage.md) and the
[machine-readable evidence](verification/t0093-linux-native.json).

## Distributed Ubuntu artifact

Ubuntu 24.04 workflow
[38141065493](https://github.com/RL-Charles/viamind/actions/runs/38141065493)
built the actual optimized application, runtime and image. Its PR source head was
`0f4ed01f2912df59c1e8d2bc183f150f4dc29647`; GitHub built synthetic merge checkout
`63fb95402c7488f3fef208cbf4977d47ff8f38c1` against main
`cc1813f2c734eb808d702002da4bcc2c93bf6bf3`. These are historical artifact
identities, distinct from later inspection/evidence changes and final CI.

Artifact `11703006622` was downloaded through the authenticated GitHub API.
The downloaded ZIP SHA256
`f4c7ad3dd3bc31ddbe3e53d0f5bda946e610d387f113f1553b405e7e099eda19`
matches the service's artifact digest. Its four bounded regular files were
extracted into a fresh directory; all distributed `SHA256SUMS` entries passed.

| Item | SHA256 |
| --- | --- |
| `viamind-0.1.0-63fb95402c74-x86_64.AppImage` | `6a98592c182c5c4f357e49bfe8d7c53122f49ec7105cd38e0052cb391af5bd3f` |
| Optimized packaged application | `d25a2582ab396471a14a4d248e287f0918fb7ff854d6fb072dffdb902b16364e` |
| Static runtime | `c962680dd2a74b1c9d86bc7f77ce5190e5ca53b5f44281617770da875d31e643` |
| Runtime source/relink companion | `5b4446da6e01d4a65841dfbfb39a898b13664621912f57b9ae9004b65d82f32d` |

The recorded build is clean, Rust 1.99.0, x86_64 GNU/Linux, glibc 2.39,
with 363 application dependency license entries. System graphics drivers,
Wayland/X11 and recorded dynamic libraries remain requirements. Successful
execution on this newer Arch/Omarchy host does not establish arbitrary Linux
compatibility. Raw provenance and the complete application file inventory are
retained in [ubuntu-appimage-info.json](verification/t0093/ubuntu-appimage-info.json).

CI executed the actual image without a display: help/version, clean stdio MCP
discovery/add/query/undo, and exact relative input/output arguments from an
unrelated directory containing spaces. It independently read back every original
application/license file, the launcher, desktop/icon files and the complete
runtime inventory. Both embedded and external companions relinked and executed
successfully using only their own retained files.

## Actual packaged desktop and shared agents

Cua launched this exact downloaded image through `--appimage-extract-and-run`
with a task-owned copy of the original asymmetric board and MCP port 7921.
Runtime PID 1204257 spawned application PID 1204342. Hashing
`/proc/1204342/exe` gave the exact packaged application SHA above. The caller's
lowercase `appdir` sentinel and selected MCP port survived the real launcher.

The portable [MCP smoke probe](../scripts/mcp-smoke.py) passed **25 checks** against
that running native application: two independent agents and a resumable observer,
shared edits/selection, exact nanometre movement, query validation, selective
undo/redo conflicts, delete/selection event ordering and pre-test design/selection
restoration. Source project bytes remained unchanged. The smoke probe does not
claim unchanged undo history. See the [raw result](verification/t0093/native-http-smoke.json)
and [actual process/window proof](verification/t0093/production-process-proof.json).

Hyprland allocated this production window at `[-6, -5]` in the crowded tiled
desktop. It is excluded from readable rendering evidence. The exact grounded Cua
capture returned a stale-target refusal and created no image. A separate agent's
Wayland diagnostic reported `out of range integral type conversion attempted`
during desktop enumeration. No successful Cua capture, native input, display
scanout or frame cadence is claimed. The
[original refusal](verification/t0093/production-cua-refusal.txt) is retained.

## Separate readable native rendering

The retained
[inspection host](../crates/ui-egui/examples/t0093_inspection.rs) uses the production
`ViamindApp`, engine and original fixture at a fixed 1600 × 1000 viewport with MCP
disabled. This is a separate local debug executable, not the packaged optimized
application. Complete `SessionSnapshot` equality is asserted after every UI pass.
During inspection it requests eframe's native WGPU framebuffer Screenshot
event and writes the original opaque RGB pixels to a NEW-only P6 artifact. It
does not imitate operating-system input or change desktop configuration.

The [readable native capture](verification/t0093/native-framebuffer.png) was
visually inspected: layer controls, two labelled components, top/bottom tracks,
via/drill rings, cyan connection guides and the asymmetric outline/cutout are
visible. The source host is `41e5156cd60f0daff26551b9652b3b359cc11864`, executable
SHA256 `deb3b15a780983527218c6b4ce049da6f40fa7af620100dc2fac622499c4ba3a`,
PID 1277767, actual viewport 1600 × 1000. Screenshot readback arrived after
11 UI passes with the complete session unchanged. UI passes are not physical
frames or a cadence measurement. The original P6 capture is retained locally;
the committed PNG preserves exactly the same RGB bytes, with conversion code
and source/output/pixel hashes retained alongside the image.

## Runtime source and relinking

The pinned Docker build was executed locally twice, including the final atomic
relink launcher. The independently checked final runtime inventory contains
170 hashed payload files plus the manifest / 142,477,086 bytes and produces the same runtime SHA as the Ubuntu
artifact. This observation does not imply general reproducibility of mutable
Alpine package resolution. Exact source archives, build recipes, package versions,
patches, static libraries, CRT/libgcc objects and licenses are distributed with
the image and external companion. The GCC archive SHA512 was also independently
checked against the pinned Alpine recipe. The runtime build log is retained.

## Gates and independent review

Local source `506f4b7` passed formatting, strict workspace all-target Clippy,
**638 workspace tests, zero failed, one existing ignored**, and the workspace
build. The two existing Linux bundle tests and eight AppImage tests passed.
Later inspection-only changes have their own strict lint/build evidence; final
combined CI includes all targets again. Full raw output is retained in
[full-linux.log](verification/t0093/full-linux.log).

Meaningful black-box tests execute the exact AppRun and distributed relink shell
launcher. They cover argument boundaries, Unicode, streams, exit status, caller
environment (including exported `appdir`), a racing existing output, corrupted
cache bytes, archive aliases/escaping/links/special files/expansion limits,
nonregular runtime manifests, provenance validation and failed-stage cleanup.

Independent review caught an exported `appdir` collision in an earlier AppRun;
the launcher now resolves its path inside command substitution without assigning
that variable, and the real process regression test passes. Final independent
review and exact-head CI are required before merge. Tickets remain in review
for human completion.
