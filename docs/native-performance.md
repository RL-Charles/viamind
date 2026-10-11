# Native Linux canvas benchmark

Run a separate native wgpu window with the production canvas and 100,000
synthetic objects:

```sh
cargo run --release -p viamind-ui-egui --example native_benchmark -- /tmp/viamind-native-report.json
```

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
