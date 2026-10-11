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
