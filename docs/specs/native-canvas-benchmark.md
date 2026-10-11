# Native canvas performance measurement

Measure the existing application's native eframe/wgpu window, using its public
camera operations and synthetic scene. Reference: the locally installed eframe
0.36.2 `App::ui`, `Frame::info().cpu_usage` and `CreationContext::wgpu_render_state`
API documentation. No external EDA implementation is involved.

The standalone `native_benchmark` example requires an output path and a release
build. It drives 100,000 synthetic objects through fitted pan, fitted pan with
continuous zoom and zoomed 8x pan phases. Each phase warms up 60 frames and samples
360 native frames. egui layout passes within one frame are deduplicated using
`Context::cumulative_frame_nr`; every native frame explicitly requests repaint. Camera animation invokes the
same pan/zoom functions used by the application's pointer handlers.

Report the actual frame-start intervals (including native loop, render/present
waits and OS scheduling), previous-frame egui CPU time, drawn object count,
actual canvas size/pixel scale and adapter/backend. Native cadence is distinct
from a GPU timestamp or physical display scanout measurement. Results include
median, p95, sustained throughput and an explicit 60 fps assessment; retain failed
measurements without claiming the acceptance check passed. The compositor may
override the requested 1600x1000 window size; report actual size. The example
closes its own window when complete and writes JSON to a newly created file.
It never attaches to or changes a user's design session.
