# Persistent GPU board tiles

Public sources: [egui-wgpu 0.36.2 callback lifecycle](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/trait.CallbackTrait.html), [wgpu buffers and uploads](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html), [egui tessellation](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.tessellate), and the MIT/Apache-2.0 [egui renderer](https://github.com/emilk/egui/tree/0.36.2/crates/egui-wgpu).

## Behaviour

- First measure production UI construction, egui tessellation, emitted vertex/index volume and native frame cadence for 100,000 visible objects. Record actual limitations; keep T20's strict 60fps criterion.
- Reuse the existing tiled tessellation and zoom bucket. Native wgpu rendering uploads each immutable tile once, then applies the camera in a vertex shader using a small uniform. Changed scene, scale bucket, layer colour/opacity or tessellation inputs rebuild the affected geometry.
- Preserve tile culling, visible layers, order, colour, premultiplied blending, anti-alias fringe, prerasterized discs, board/grid background and overlays. No geometry or objects may be hidden to meet the benchmark. Texture data for geometric white/disc atlas regions comes from the public font image API.
- Only enable custom callbacks when the native wgpu target format is supplied. Headless tests and other renderers keep the existing egui mesh path. Cached and immediate rendering remain available as references.
- Resources belong to one canvas and are dropped when tiles are replaced; scene edits must not grow GPU memory indefinitely. Native callback geometry must remain immutable through egui layout passes and concurrent renderer preparation.
- Verify the camera transform and invalidation rules plus native/reference image parity. Run the existing unmodified three-phase native benchmark after optimization, preserving cadence thresholds and full visible-object counts.

## Limits

The existing tessellation cache permits at most 9% anti-alias fringe scaling inside a zoom bucket. The optimization retains that policy. Frame-start cadence includes OS scheduling/presentation and is not a GPU timestamp measurement. CPU profile stage timings exclude GPU execution.
