# Persistent geometry buffers per board layer

Public sources: [egui-wgpu callback lifecycle](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/trait.CallbackTrait.html), [wgpu queue uploads](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html), [wgpu vertex buffer layout](https://docs.rs/wgpu/30.0.1/wgpu/struct.VertexBufferLayout.html), and [WGSL storage buffers](https://www.w3.org/TR/WGSL/#storage-address-space).

## Behaviour

- Retain T50's tessellated tile geometry, camera mapping, atlas, colour/blending, zoom bucket, culling and layer order. The headless egui mesh reference remains available; native devices without vertex storage buffers also select that reference. No design commands or agent capabilities change.
- Consolidate each layer's built tiles into one immutable vertex/index buffer pair. Attach a stable transform slot to each vertex; rebase indices while preserving tile and triangle order. Replacing geometry replaces the affected layer's pair. Previously built, currently culled tiles may remain resident until invalidation; their indices must not be drawn.
- Upload a packed array of tile transforms once per frame and a separate viewport uniform. Native uploads allocate staging memory, so avoid per-tile buffer writes and per-tile buffer allocation. Bind the layer once and merge only directly adjacent visible index ranges. Never merge over a hidden tile's indices.
- Keep geometry resources owned by cache generations and callback references. Old frames may retain the old generation; after those references disappear, handles must be released without a global resource map.
- Test index rebasing, exact vertex attributes and transform slots, culling gaps, camera-only reuse, invalidation and replacement lifetime. Run the existing native screenshot parity and unchanged 100,000-object benchmark against the same binary's egui reference. Record actual cadence and preserve the strict 60 fps target.

## Limits

Rebuilding a layer still serializes and uploads its resident geometry, and zoom bucket crossings still tessellate visible tiles. Batching reduces allocation and draw overhead; measured improvement is required before claiming a cadence benefit. Native verification uses the existing single-sample, default-filtering, 1 pixel/point configuration; other DPI/sample/depth configurations remain unverified as in T50. No geometry is hidden to improve timings.
