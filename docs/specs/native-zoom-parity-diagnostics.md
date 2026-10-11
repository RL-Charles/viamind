# Bounded diagnosis and correction of populated-zoom canvas parity

Ticket T-0126 / S-017. Public contract for independent review before Rust.
Initial diagnostic base is merged `e7f60cf489ddb673ad416f991fb86c398c61a2ed`.
Final integration preserves later merged features and runs exact-head CI.

## Observed failure and public sources

One unchanged release run of the existing `gpu_parity` example failed only its
fifth, populated 8x case: 581 drawn objects, mean channel difference 6.9391245,
maximum 215, and 346026 of 1068480 canvas pixels with any channel difference >2.
Cases 0..3 had maximum differences 1, 1, 1, 2 and zero pixels >2. The example
returned failure and self-closed. The wrapper therefore never launched the 100k
cadence example. Original source, binaries, reports, scripts, quiet admission,
build log and system state are retained; this is not a performance result.

The compatible open-source [egui 0.36.2 Context documentation](https://docs.rs/egui/0.36.2/egui/struct.Context.html)
distinguishes completed native integration frames from repeated UI layout passes.
Its [ViewportCommand::Screenshot contract](https://docs.rs/egui/0.36.2/egui/viewport/enum.ViewportCommand.html#variant.Screenshot)
captures the next frame and returns an [Event::Screenshot](https://docs.rs/egui/0.36.2/egui/enum.Event.html#variant.Screenshot)
with viewport identity, caller user data and an image. Those public contracts
support uniquely identified asynchronous readbacks and frame/pass provenance.
No source establishes the cause of this Viamind failure in advance.

The existing CPU reference is the cached egui mesh path, not uncached scene
tessellation. Persistent native GPU geometry and that reference must be compared
as such. A camera/layout mismatch, stale or transformed cached geometry, texture
state, and a primitive-specific raster mismatch remain hypotheses until measured.
The large fixture via is retained and receives explicit region diagnostics;
the mismatch is not presumed to affect all primitives or to be a harness defect.

## Diagnostic behavior and bounds

The first diagnostic-only trace retains the legacy example's transition semantics
and records raw pairs and repeated-pass/camera transitions before any hardening.
It flags invalid provenance explicitly and cannot claim such a pair as valid.
The final hardened diagnostic mode enforces token/deduplication and stable-pair
requirements below only after causal evidence review. This ordering prevents
instrumentation from concealing a harness failure before it can be explained.
Both stages have the same image/work bounds, objects and pixel criteria.

1. Preserve the original failed report and both original release binary hashes.
   Add optional diagnostics to the existing five-case example, retaining the same
   synthetic 5000 scene, added via, quad, ratsnest, layer operations, camera
   operations, default native DPI, requested 1400x1000 viewport, and thresholds.
   Existing no-diagnostics report fields and pass computation remain compatible.
   No object or failing primitive is removed to obtain a pass.
2. A diagnostic output directory is explicitly supplied and must be NEW. Use
   create-new files and fsync completed reports/images. Never overwrite old proof.
   Reject invalid/pre-existing destinations before launching the native window.
   Retain the original full RGBA samples (P7 or an equivalent lossless encoding),
   not only a composited or alpha-stripped review image. Optional RGB PNGs disclose
   alpha omission and are verified against every original RGB sample.
3. Use a typed unique screenshot token containing case, renderer and request
   sequence. Match the returned viewport/token to one outstanding request. Reject
   unknown, out-of-order or mismatched readbacks with a diagnostic failure rather
   than accepting a different image. A duplicate callback from another UI pass
   cannot consume the same token twice or advance the case/camera twice.
4. Record cumulative frame number, cumulative pass number, current pass index,
   request and arrival identities, renderer choice, camera center/zoom, viewport,
   pixels per point, screenshot dimensions, visible item count and cache counters.
   Freeze one case camera/layer state through both renderer readbacks. Because the
   public screenshot contract captures the next frame, retain the relevant draw
   receipts through arrival; do not label request-frame state as captured state
   without proving they agree. Multiple layout passes remain possible and are
   recorded distinctly from native rendered frames.
5. Before comparing, require matching pair dimensions, camera/layer configuration,
   viewport pixel crop and DPI. Report a provenance failure separately from a
   valid-image pixel mismatch. Do not silently resize, re-register, shift, mask,
   color-convert or crop away geometry to manufacture equivalence. If layout
   changes a camera via auto-fit, record it and establish a stable pair explicitly.
6. The normal five whole-canvas comparisons remain authoritative. Add bounded
   mismatch bounding boxes, changed-pixel counts and channel summaries for the
   fixture via/drill, quad and other visible primitive regions. Region overlap
   must be reported; region counts are diagnostics, not replacement thresholds.
   Independent inspection of retained paired images determines the affected
   geometry before claiming a cause. The unchanged guides/grid remain present.
7. Any additional cache diagnostics are read-only: bounded tile/layer counts,
   immutable source/build bases, current/applied transforms, generation lineage,
   mesh vertex/index counts and optional deterministic finite fingerprints.
   Distinguish cached CPU transformed vertices from persistent GPU source and
   layer packing. Do not mutate cache lifetimes or the scene merely to collect data.
   A narrow internal inspection API is acceptable; no new product UI/MCP action.
8. Exactly ten successful images cover five pairs. At most one outstanding native
   request; at most two full images in memory. Enforce checked dimensions <=16M
   pixels/image and <=64MiB raw/image, <=640MiB total raw artifacts, <=900 native
   frames, <=180 seconds, and <=1MiB metadata before allocation/copy where the
   integration has not already allocated the screenshot. Unexpected extra images
   are refused. A bounded failure report explains missing/unmatched captures;
   close the owned diagnostic host after fsync. Preserve older previews/daemon.

## Correction and verification

9. First run one bounded diagnostic pair sequence on the exact failure source plus
   diagnostic-only changes. Establish and independently review the causal evidence
   before changing renderer behavior. A harness correction must have a focused
   repeated-pass/token/next-frame regression demonstrating why the old sequence
   compared the wrong state. A renderer/cache correction must have a focused
   independent geometry/lifetime/transform or raster regression demonstrating the
   faulty behavior and the exact fix. No speculative performance rewrite.
10. Correct only the demonstrated cause, preserving persistent GPU geometry,
    layer order/visibility/opacity, culling, native/headless reference semantics,
    arbitrary existing geometry and default scene behavior. Retain the original
    same five parity cases and object counts. If evidence requires wider scope,
    amend the public contract and obtain independent review before that change.
11. Final native paired readback must pass all five original whole-canvas criteria:
    mean channel difference <0.1 and fewer than `pixels/1000` pixels differing >2.
    Also report the historical stricter observation (maximum <=2 and zero pixels
    >2) without replacing either receipt. Preserve failing-before/passing-after
    source, unique executable hashes, frame/camera metadata and original pixels.
    Default display/DPI and actual physical monitor/host load are disclosed.
12. Focused tests cover the demonstrated defect and diagnostic bounds/provenance.
    All four Linux workspace gates, generated tool/workflow drift checks, and
    exact-head CI pass before submission. Keep compilation under shared mutexes,
    touch current workspace Rust inputs before normal builds, and retain unique
    native binaries before releasing a shared target. Coordinate quiet native
    capture windows with other lanes; no unrelated repeat benchmarks.
13. This ticket establishes visual correctness, not native 100k performance. T20's
    60fps acceptance remains open. Only after visual correctness is restored may
    its separately coordinated ONE ordinary current baseline run proceed with
    100000 objects, 1260 native frames, unchanged three phases, >=59.5fps and
    p95<=16.9ms. GPU timestamp diagnostics, screenshots or extra profiling are not
    part of that ordinary timing run. No human Done transition by an agent.
