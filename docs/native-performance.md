# Native Linux canvas benchmark

Run a separate native wgpu window with the production canvas and 100,000
synthetic objects:

```sh
cargo run --release -p viamind-ui-egui --example native_benchmark -- /tmp/viamind-native-report.json
```

Append `--reference` to measure the egui mesh tile path with exactly the same
scene, phases and acceptance thresholds as the native GPU tile path. Reports
identify the selected `canvas_renderer`.

Use a new output filename. The example refuses debug builds and existing files,
automatically pans/zooms through three phases, and closes when its JSON report is
complete. It operates on a synthetic scene independently of open design sessions.
Each phase warms up for 60 native frames and measures 360 more. Multiple egui
layout passes count as one frame. The report identifies the adapter, actual
canvas size and pixel scale; the compositor may override the requested window
size. The benchmark requests a fixed 1600 × 1000 window to avoid a crowded tiling
layout reducing the canvas to a narrow strip. Always inspect the actual canvas
size in the report before using its results.

`frame_interval` includes native render/present waits and scheduling between
frame starts. `egui_cpu` is eframe's previous-frame CPU usage. Neither is a GPU
timestamp or measurement of physical display scanout. `meets_60fps` requires at
least 59.5 frames/second sustained and p95 frame intervals at most 16.9 ms. Keep
failed results; they identify real work remaining. Record machine/display,
compiler/build revision and concurrent load alongside benchmark evidence.

The existing ignored `tests/perf.rs` benchmark measures UI construction and CPU
tessellation without a native window. It helps diagnose bottlenecks but does not
replace this native check.

## Initial Linux measurement (2026-10-10)

Production canvas at main `20e9910` plus this example, optimized Rust build,
Intel Core Ultra 7 255U (14 logical CPUs), Intel Graphics (ARL) through Vulkan,
Mesa 26.2.2, Hyprland, 1920 × 1200 display at 60.002 Hz, scale 1. Actual canvas:
1320 × 954 points. Three independent design previews and parallel development
builds remained running; load average was approximately 5.9. These are observed
desktop conditions, not an isolated hardware ceiling.

| Phase | Objects drawn | Median interval | p95 interval | Sustained fps |
| --- | ---: | ---: | ---: | ---: |
| Fit pan | 100,000 | 33.35 ms | 50.00 ms | 28.24 |
| Fit pan and zoom | 80,311–100,000 | 33.40 ms | 50.19 ms | 26.09 |
| Zoomed 8× pan | 1,928–1,965 | 16.66 ms | 16.91 ms | 60.00 |

All three phases fail the strict criterion above. The original 100k/60fps
acceptance remains open. Each phase collected 360 samples after 60 warmup frames;
the run completed 1,260 native frames (1,261 egui passes). Fit-pan previous-frame
CPU median was 21.65 ms; pan-and-zoom CPU median was 24.94 ms. Full-board rendering
requires optimization before claiming 60 fps.

Local evidence is retained in `~/.local/state/viamind/recovery/`:
`native-100k-d2f5a14-fixed-run2.json`, `native-100k-fixed-window.json`, and
`native-100k-fixed-window.png`. The first report,
`native-100k-38c40eb-run1.json`, is also retained: its 19 × 522 canvas was too
narrow to represent the intended full-board viewport.

## Persistent native GPU tile buffers (T50)

The production CPU stage profile (`cargo run --release -p viamind-ui-egui
--example cpu_profile`) separates UI construction from egui mesh assembly. At
1320 × 954 points, panning a 100k scene averaged 1.805 ms UI construction and
11.802 ms tessellation/mesh assembly, producing 704,834 vertices and 2,274,186
indices: approximately 23.19 MB copied into the renderer each frame. The profile
excludes GPU upload/render/present and panning left 99,089 objects visible. It is
a diagnostic, not a replacement for the native benchmark.

The initial stage profile and isolated results below precede the merged T45
round track caps; they are retained as historical evidence. Final comparisons
use `--reference` on the same integrated binary.

Native launch now explicitly configures a persistent GPU tile path. Existing
egui-tessellated vertices, indices and geometric font-atlas regions are uploaded
once per tile generation. A single batched upload supplies camera uniforms;
layer order, tile culling and the 9% zoom bucket remain the same. Headless tests
and unconfigured apps retain the egui mesh reference path. `cache.disable_gpu()`
selects that reference for native screenshot comparisons.

The first isolated persistent-buffer run on the same Linux/Vulkan device and
1320 × 954 canvas produced:

| Phase | Sustained fps | p95 interval | Mean previous-frame CPU |
| --- | ---: | ---: | ---: |
| Fit pan | 60.000 | 16.946 ms | 3.835 ms |
| Fit pan and zoom | 46.055 | 17.168 ms | 8.524 ms |
| Zoomed 8× pan | 60.002 | 16.892 ms | 1.380 ms |

**The strict full-board 60fps acceptance remains open.** Fit pan narrowly exceeds
the 16.9 ms p95 threshold; fit pan-and-zoom still stalls while replacing many GPU
tile buffers at zoom bucket boundaries. Consolidating those allocations is a
follow-up optimization. No thresholds, visible objects or phase counts changed.
Parallel development checks and idle design previews remained running; other
benchmark windows had completed before this isolated run.

`gpu_parity NEW_REPORT.json` renders four native canvas comparisons against the
egui mesh reference: fit, pan, layer hide/reorder/opacity and zoom inside a bucket.
Across 1,068,480 pixels per comparison, maximum channel difference was 2/255,
zero pixels differed by more than two channel values, and mean channel difference
was below 0.0007. This includes the same antialiasing and prerasterized discs.
Run it with `cargo run --release -p viamind-ui-egui --example gpu_parity --
/tmp/new-parity-report.json`. The fixed-size synthetic window closes itself.

Machine-readable evidence is in [performance/](performance/). Failed exploratory
runs (per-tile uniform uploads and overlapping benchmark windows) remain in the
local recovery directory; they are not used to claim acceptance.

### Final integrated same-binary comparison

At source `4e60cfb1ce919485159437a3b62a430f4e20bf03` (based on merged
copper/placement main `b01f96b`), the same saved release binary rendered both
paths sequentially at 1320 × 954 points, with the same 1,260 native frames and
phase object counts. Load average observed between runs was 13.23: Viamind debug
gates, unrelated hypervisor compilation/test workloads and open native previews
were active. These are desktop observations, not controlled hardware limits.

| Phase | Reference fps | GPU tiles fps | Reference mean CPU | GPU tiles mean CPU |
| --- | ---: | ---: | ---: | ---: |
| Fit pan | 13.317 | 22.930 | 36.184 ms | 5.577 ms |
| Fit pan and zoom | 12.821 | 16.836 | 40.515 ms | 14.724 ms |
| Zoomed 8× pan | 32.925 | 30.727 | 4.057 ms | 1.983 ms |

All strict cadence checks fail under these conditions. Reduced previous-frame
CPU and full-board cadence improvement support the cache optimization; the
small visible-set cadence variation demonstrates why CPU timings alone do not
prove native 60fps. The original acceptance remains open.

Integrated native screenshot parity includes a through-via hole, rotated copper
quad and connection guide overlay alongside the 5,000 synthetic items. All four
cases pass: maximum difference 2/255, no pixels exceed two channel values, and
mean channel difference is below 0.000265 across 1,068,480 pixels per case.
Reference/GPU reports, parity, CPU profile and source/binary metadata are retained
in [performance/](performance/). The final CPU diagnostic averaged 2.210 ms UI
and 17.486 ms egui mesh assembly under this heavier load, with unchanged geometry
volume of 23.19 MB/frame in the reference path.

Native verification uses default single-sample wgpu, no depth attachment,
1 pixel/point and default linear atlas filtering/dithering. Alternate custom
sample/depth/filter policies and DPI are unverified in this increment. The public
API keeps the egui mesh reference available for those integrations.

## Persistent GPU buffers per layer (T55)

T55 packs resident tile geometry into one immutable vertex/index buffer pair per
layer. Each vertex retains a stable tile transform slot. A packed storage buffer
supplies transforms once per frame, with a separate viewport uniform. Draws merge
only adjacent visible index ranges, preserving gaps over culled tiles and the
existing layer order. Native devices without vertex storage support retain the
egui mesh path. The same egui geometry, atlas, zoom bucket and overlays remain.

The five native image cases include a populated 8× view with 606 visible objects,
alongside fit, pan, layer visibility/order/opacity and zoom inside a bucket.
Across 1,068,480 pixels per case, maximum difference is 2/255, no pixels exceed
two channel values and mean channel difference stays below 0.000265. Packing
tests also verify every vertex attribute, transform slot and rebased index;
cache tests verify camera-only reuse and release of replaced layer generations.

Zoom bucket crossings still tessellate visible tiles and upload the affected
layer's resident geometry. Previously built, currently culled tiles may remain
resident until invalidation, but their indices are excluded from draws. A layer
must fit within the device maximum buffer size; extremely large layers need
future subdivision. Native verification retains T50's default DPI/sample/filter
limits, and the original strict 60fps acceptance stays open until measured.

### Sequential same-geometry measurements

Saved release binaries ran sequentially on Linux/Vulkan at 1320 × 954 points,
1 pixel/point, with 1,260 native frames each. All phase object-count bounds match:
100,000 throughout fit-pan, 80,311–100,000 for fit-pan-and-zoom, and 1,928–1,965
for zoomed pan. Source/binary identities and reports are in
[performance/](performance/). The reference and layer paths use the same T55
binary; the saved T50 binary provides the preceding tile-buffer comparison.

| Phase | Mesh reference fps | T50 tiles fps | T55 layers fps | Reference mean CPU | Tiles mean CPU | Layers mean CPU | Layers p95 interval |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fit pan | 9.769 | 14.477 | 23.404 | 55.077 ms | 9.352 ms | 1.600 ms | 51.734 ms |
| Fit pan and zoom | 10.345 | 10.589 | 22.223 | 50.631 ms | 22.120 ms | 3.285 ms | 65.454 ms |
| Zoomed 8× pan | 26.993 | 30.210 | 30.044 | 7.211 ms | 2.258 ms | 2.348 ms | 38.605 ms |

**All strict cadence checks fail; the original native 100k/60fps acceptance
remains open.** Full fit uses exactly 10 indexed draws and 10 layer buffer pairs,
in place of approximately 2,000 tile pairs. Pan-and-zoom uses 9–110 visible
ranges while preserving culling, and zoomed pan uses 22–24 ranges. Camera-only
phases build zero tiles after warmup; the zoom phase can rebuild up to 1,729 tiles
at a bucket crossing. Lower CPU time supports this allocation/draw optimization,
while frame cadence still includes GPU execution, presentation and scheduling.

Observed load averages ranged from 11.28 to 18.20 during these runs. Unrelated
CPU tests/compilation and existing native design previews remained active; the
Viamind builds for this ticket had completed. One window snapshot was captured
during the reference run; no input or screenshot actions occurred during either
GPU timing run. These sequential desktop observations are not a controlled
hardware ceiling or a GPU timestamp measurement. Host load varied between runs,
so the fps differences must not be interpreted as an isolated causal estimate.

## Optional native GPU command timing (T59)

The native benchmark accepts `--gpu-profile NEW_GPU_REPORT.json` after its
ordinary cadence output path. This explicitly requests timestamp-query features
when supported, leaving ordinary application/benchmark device requests unchanged.
Unsupported devices write an unavailable diagnostic. GPU durations remain
separate from the unchanged strict frame-cadence criterion:

```sh
cargo run --release -p viamind-ui-egui --example native_benchmark --   /tmp/new-cadence.json --gpu-profile /tmp/new-gpu.json
```

Four reusable query/resolve/readback slots provide asynchronous previous-frame
readback during measurement; no blocking GPU wait runs inside measured frames.
A final drain waits at most 10 seconds after cadence collection ends. Callbacks
capture frame/phase tags, discarded reservations release their slots, and stale
completions cannot release newer generations. Raw history is bounded to 4,096
samples; busy, discarded, invalid/readback and overflow counters remain visible.

The native Intel Graphics (ARL), Vulkan/Mesa 26.2.2 diagnostic completed with all
1,260 tagged GPU samples and 360 sampled durations per phase. Every expected
frame ID appears once. Busy/discarded/readback/overflow counters, final errors
and pending slots are zero; the final drain succeeded. Queue timestamp period
was 52.083332 ns. Evidence is in `performance/t0059-*.json`.

| Phase | GPU mean | GPU median | GPU p95 | Instrumented native fps |
| --- | ---: | ---: | ---: | ---: |
| Fit pan | 23.471 ms | 21.952 ms | 31.388 ms | 25.654 |
| Fit pan and zoom | 25.718 ms | 22.084 ms | 45.827 ms | 22.315 |
| Zoomed 8× pan | 9.277 ms | 6.876 ms | 18.628 ms | 29.510 |

These are elapsed GPU timestamps immediately around the production board draw
commands. They exclude UI/background/overlays, uploads, presentation and queue
waiting before the first timestamp. GPU context interference between timestamps
and diagnostic overhead can affect durations. Observed host load increased from
13.53 to 16.48; other CPU workloads and existing native previews remained active.
Viamind builds had finished, and no screenshot/input actions occurred during
measurement. This is evidence of remaining GPU cost under those conditions, not
an isolated shader or hardware ceiling. Instrumented cadence is diagnostic;
uninstrumented T55 cadence remains the acceptance evidence. All strict cadence
checks still fail, and the original 100k/60fps acceptance remains open.


## T62 — shared uniform camera basis

Verified implementation source `3d0ca54dcb1817652c15832a5ac7a9f6facab491` packs immutable tiles into one camera basis per layer. Vertices now occupy 20 bytes instead of 24: the tile-slot attribute and vertex storage-buffer fetch are removed. One aligned, batched uniform upload supplies all visible layer transforms, selected with dynamic offsets. Camera differences use nearby world centers; translations beyond 1,024 points and the unchanged zoom bucket rebase from original tessellation. Old generations retain their existing cache/callback ownership. The renderer no longer requires vertex storage support; the headless/default CPU reference remains available.

The actual native five-case image comparison passed: 1,068,480 pixels/case, maximum channel delta 2/255, zero pixels above 2 and populated 8x culling (606 drawn objects). Precision checks compare the composed and direct tile CPU transforms within 0.001 point over the documented tested range; they do not guarantee nanometre accuracy at extreme world coordinates through the existing f64 scene boundary.

The saved T59 baseline and new T62 GPU diagnostics ran sequentially on the same Intel Vulkan adapter, followed by ordinary T62 GPU cadence and the same binary's CPU reference. Both diagnostic reports are complete: 1,260 unique frame tags, 360 sampled durations/phase, four bounded query slots and zero errors, counters or pending slots after drain.

| Phase | T59 GPU mean / p95 ms | T62 GPU mean / p95 ms |
| --- | ---: | ---: |
| fit_pan | 34.938 / 36.587 | 33.601 / 36.338 |
| fit_pan_zoom | 39.940 / 65.026 | 42.137 / 79.841 |
| zoomed_8x_pan | 17.519 / 32.011 | 14.472 / 19.482 |

GPU results are mixed. The full-pan mean is slightly lower and the 8x mean lower, while pan/zoom is higher. These sequential measurements under varying host activity do not establish a general or isolated causal speedup. GPU elapsed durations can include context interference between timestamps and query overhead; instrumented cadence does not replace ordinary acceptance evidence.

| Phase | CPU reference fps / p95 ms | T62 GPU fps / p95 ms | CPU reference / T62 frame CPU mean ms | Strict native gate |
| --- | ---: | ---: | ---: | --- |
| fit_pan | 11.502 / 101.260 | 17.792 / 80.414 | 31.226 / 1.841 | fail |
| fit_pan_zoom | 10.965 / 115.680 | 16.390 / 86.374 | 35.503 / 3.294 | fail |
| zoomed_8x_pan | 22.155 / 51.894 | 20.611 / 51.723 | 4.007 / 1.480 | fail |

All strict checks remain false: the unchanged criterion is >=59.5 fps **and** p95 <=16.9 ms. Original native 100k/60fps acceptance remains open. Every run retained 60 warmup plus 360 sampled frames per phase, 1,260 native frames, a 1,320x954 canvas at one pixel/point, fit-pan count 100,000, pan/zoom 80,311..100,000 and 8x 1,928..1,965. The GPU full-fit path still uses ten layer draws; pan/zoom 9..110 ranges and 8x 22..24 ranges preserve culling gaps. Full pan/8x have no warm tile rebuilds, pan/zoom peaks at 1,729 as before.

Observed one-minute loads ranged 8.32..15.29. Other CPU workloads and existing native previews remained active. Own builds finished before timing; no screenshots or input actions occurred during the four timing runs. Verification covers native Intel Vulkan at default DPI/single sample; other adapters, backends and settings remain unverified. Geometry changes and precision rebases still repack resident layers, and enormous layers still require subdivision to satisfy device buffer limits.

[Raw metadata](performance/t0062-metadata.json), [image parity](performance/t0062-parity.json), [ordinary GPU cadence](performance/t0062-cadence.json), [CPU reference](performance/t0062-reference.json), [baseline GPU diagnostics](performance/t0062-baseline-gpu.json), [new GPU diagnostics](performance/t0062-profile-gpu.json), [baseline instrumented cadence](performance/t0062-baseline-cadence.json), [new instrumented cadence](performance/t0062-profile-cadence.json). All Linux local gates passed: fmt, strict all-target clippy, 315 tests, 0 failed, 1 existing ignored, workspace build and release examples; source CI also passed.
