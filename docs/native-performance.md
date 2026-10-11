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
size.

`frame_interval` includes native render/present waits and scheduling between
frame starts. `egui_cpu` is eframe's previous-frame CPU usage. Neither is a GPU
timestamp or measurement of physical display scanout. `meets_60fps` requires at
least 59.5 frames/second sustained and p95 frame intervals at most 16.9 ms. Keep
failed results; they identify real work remaining. Record machine/display,
compiler/build revision and concurrent load alongside benchmark evidence.

The existing ignored `tests/perf.rs` benchmark measures UI construction and CPU
tessellation without a native window. It helps diagnose bottlenecks but does not
replace this native check.
