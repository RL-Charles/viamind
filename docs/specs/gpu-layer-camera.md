# Shared camera basis for persistent GPU layers

Ticket: T-0062. Independent implementation from the public APIs below.

## Public sources

- [wgpu 30.0.1 BufferBinding](https://docs.rs/wgpu/30.0.1/wgpu/struct.BufferBinding.html): a uniform binding may select an aligned segment of a buffer; dynamic offsets add to the binding offset.
- [wgpu 30.0.1 RenderPass::set_bind_group](https://docs.rs/wgpu/30.0.1/wgpu/struct.RenderPass.html#method.set_bind_group): render passes select the uniform segment through dynamic offsets.
- [wgpu 30.0.1 Limits](https://docs.rs/wgpu/30.0.1/wgpu/struct.Limits.html): uniform buffer alignment is a device limit.
- [egui 0.36.2 Mesh](https://docs.rs/epaint/0.36.2/epaint/struct.Mesh.html): indexed vertices retain positions, texture coordinates and premultiplied colors. Existing shader color conventions retain their attributed license.

## Behavior

A resident layer generation packs each tile's immutable tessellation into one common screen camera basis. The packed vertex contains the same position, UV and color attributes, with no tile transform index. Indices and visible index ranges preserve the existing order and gaps. The vertex shader uses a single uniform affine camera transform for the whole layer. All layer camera records are uploaded together once per callback, padded to the device's required uniform alignment; a draw selects its record with a dynamic offset.

Camera mappings and changes of basis are calculated with f64 before the render boundary. Every tile retains its original tessellation and build camera, so rebasing always starts from immutable geometry and never compounds earlier packed rounding. The layer basis is replaced when the camera moves too far or changes scale beyond the existing zoom bucket. Far resident tiles remain culled. Visible tiles whose old tessellation basis is too far from the current camera are rebuilt before reuse, preventing cancellation of large screen coordinates. Precision tests include large world origins and returning to previously resident regions.

The CPU mesh reference, shape generation, visibility, layer order, atlas sampling, antialiasing zoom bucket, overlays and scene invalidation remain available. Geometry buffers belong to a layer generation and are released when the cache and in-flight callbacks release it. Ordinary rendering uses no GPU timestamp features; optional T59 diagnostics remain separate.

## Verification and limits

Tests cover packed attributes, indices, culling gaps, repeated transforms and rebases, generation replacement and release. Native CPU/GPU image comparison retains the existing populated five cases and pixel thresholds. The 100k benchmark retains its canvas, scene, actual object counts, warmup/sample counts and strict 59.5 fps plus p95 <= 16.9 ms criterion. Save unique verified binaries and record source, adapter, host load and raw reports. Compare optional GPU elapsed durations separately from uninstrumented native cadence. No 60fps claim is permitted without passing the unchanged actual cadence gate. Linux default native DPI/sample count is the verified platform configuration; other adapters and settings require their own native evidence.

Camera rebasing uses a 1,024-point translation bound and the unchanged f64 zoom bucket. The quantitative unit check compares original tile transforms and composed layer transforms for 1,000 updates, base coordinates -512..3,072 points and world centers +/-8e12 mm at zoom 0.1, 10 and 100,000; the difference must stay below 0.001 point. This is relative render accuracy, not a guarantee of nanometre accuracy at enormous world coordinates: the existing f64 scene boundary still quantizes such positions. Arbitrarily enormous primitive extents and other DPI/sample configurations are outside this measured bound. Native image parity additionally checks the actual shader and viewport mapping.
