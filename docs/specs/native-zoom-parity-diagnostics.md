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

## Amendment: one deferred-I/O comparison after unreproduced failure

The first bounded legacy trace on diagnostic source
`ce135b238667993f5bf215254389fd71958d4635` passed all five comparisons with
valid recorded pair provenance. Its first four results exactly match the original
failure run, but case 4 drew 606 objects rather than 581. Physical window size,
monitor and DPI agree. Original camera/pass receipts do not exist, so this pass
does not explain or correct the historical failure. No renderer behavior changed.
The retained fixture via and quad are offscreen in this trace's case 4; that fact
does not remove them or weaken the whole-canvas comparison.

After independent approval of this amendment, add a separate diagnostic-only
deferred-I/O variant and perform at most ONE stated comparison capture. This is
not permission for retries until passing or for a renderer correction.

- First run bounded headless tests through the actual production app and cache
  render-mode sequence: the original five case operations, fixed 1400x1000 input,
  and controlled one-pixel viewport perturbations. Limit each test to 120 native
  input steps with the unchanged 5002-item fixture. Record/assert actual cameras
  and viewports. Programmatic camera edits currently leave startup auto-fit active;
  demonstrate whether a resize can reset that camera. This establishes a possible
  state hazard, not proof that the original native failure used it. No native
  raster or performance claim follows from a headless test.
- The optional deferred variant retains at most ten screenshot `Arc` handles and
  ten fixed image receipts in precharged slots. Shared reference/current-event
  handles do not copy their pixel buffers. Charge each unique retained image's
  checked raw byte size before retaining it: <=16M pixels and <=64MiB per image,
  <=640MiB in aggregate. This explicitly replaces the initial variant's two-image
  retention limit only for this deferred comparison. Framework-owned readback
  allocations remain outside the diagnostic's ownership claim.
- During the legacy case sequence, perform no raw image or JSON serialization,
  file writes or fsync. Create/validate the NEW destination before native launch.
  Defer image/receipt serialization until all ten arrivals or a terminal abort;
  stop case advancement before flushing. Flush/fsync each original RGBA image and
  its receipt, then release its handle. Preserve the original small final parity
  report behavior and original tokens, transitions, cameras, layers and criteria.
- Add only fixed, bounded camera-relevant input observations per UI pass: counts
  of pointer motion/button, wheel/zoom and Home/Space events, scroll/zoom deltas
  and relevant held buttons/keys. Read borrowed input; never record text, clipboard
  contents or arbitrary key strings. Refuse >100000 input events in one pass before
  scanning/retention. Charge these typed slots before allocation within the same
  combined 1MiB retained-plus-serialized metadata budget. Keep ten-image, one-request,
  900-frame, 180-second and reserved bounded failure-report limits unchanged.
- Meaningful tests cover deferred ownership/admission and flush-on-abort, no
  before-finish image/JSON files, count/byte caps before retaining an extra image,
  and exact raw samples after final flushing. Retain existing duplicate-event,
  repeated-pass token, both-direction extent and metadata regressions.
- Before that ONE native comparison, retain a unique scoped-verified binary and
  obtain fresh quiet admission under both shared mutexes. Preserve the original
  failure and initial passing trace. Independently inspect all raw pair/provenance
  receipts before considering a correction. A repeated pass still leaves the
  historical cause unresolved; stop after the single comparison and report that
  limitation. No cadence run or visual-correctness closure follows automatically.

## Amendment: independently timed progress receipt after incomplete comparison

The ONE deferred comparison on `9783c974b83f42938408e9f0d4b85761665eabd5`
did not self-close within the external 210s watchdog. No terminal trace or pixel
files were flushed, and the created parity report stayed empty. The exact-window
snapshot shows case-3-like geometry; internal step/request state is unknown.
The initial run took about 20s. This timing difference is an observation, not
causality. The owned helper was retired after exact identity revalidation, with
no replacement launch. Original failure and initial passing pixels remain intact.

Pinned compatible egui-wgpu source unmaps its capture buffer before sending an
owned CPU `ColorImage`; retaining the later image `Arc` does not retain that
mapped buffer. This excludes that specific ownership explanation, not a backend
stall. The existing 180s guard executes inside UI callbacks and cannot report
when those callbacks stop. One non-elevated debugger attachment and readable
kernel stacks were refused; no stack or renderer defect has been established.

The diagnostic question for a possible later run is narrow: **at the deadline,
has the UI callback returned with an outstanding screenshot, or is execution
still inside a particular diagnostic/app callback phase?** It does not ask for
another passing parity attempt. Add the following only after independent review:

- In an explicit progress-enabled deferred mode, precharge one fixed shared
  structure of at most 32 atomic numeric fields, no more than 512 bytes, before
  thread creation. Fields include callback entries/exits, legacy step, settle
  count, screenshot requests prepared/submitted, screenshot arrivals, pending
  token sequence, latest already-observed frame/pass, terminal classification,
  and a fixed numeric phase enum. Phases identify budget check, screenshot
  extraction, receipt admission, legacy comparison, app drawing, trace drawing,
  screenshot submission, repaint request, return and terminal flushing.
  Do not retain more event/text data, image handles, strings or unbounded history.
- Write these observations at existing boundaries. No new GUI/backend locks,
  render operations, scene changes or case transitions are introduced. Do not
  call Context merely to obtain another observation when an existing frame
  receipt is available. Explicitly disclose the added atomic operations and
  watchdog thread as observational overhead; a changed outcome remains possible.
- Use one independent standard thread and a capacity-one terminal notification.
  It waits at most 180s using the public standard-library
  [Receiver::recv_timeout](https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout)
  contract. UI callbacks never wait for the thread, perform a sequence write,
  poll the thread, or send more than one terminal notification. The watchdog
  does not request redraw, invoke GUI APIs, move the camera or close the window.
- The worker reads only this fixed atomic structure after terminal notification,
  sender disconnection or timeout, never from the image/trace vectors. Safe
  [atomic sharing](https://doc.rust-lang.org/std/sync/atomic/) supplies individual
  observations, not an automatically coherent aggregate. Use bounded generation
  reads to mark a snapshot inconsistent if a callback changes it or is active;
  never spin waiting for the GUI. Inconsistent fields remain diagnostic hints.
- Only then serialize one NEW `progress.json` receipt of at most 8KiB, with fsync,
  numeric counters, fixed phases, elapsed time and terminal/timeout reason.
  Reserve its fixed retained state and output allowance within the existing
  combined 1MiB metadata budget before allocating. The immutable output path
  stays in the already admitted NEW trace directory. No image copies or extra
  screenshot requests are permitted. No writes occur during the active case
  sequence before a timeout/terminal state. A terminal thread join may occur
  after native event-loop return, outside UI callbacks; the independent 210s
  external watchdog remains authoritative if callback or writer completion fails.
- Tests use injected short deadlines and synthetic counter states to prove a
  receipt appears without further UI callbacks, pending-request/inside-callback
  states differ, terminal/disconnect classification is exact, generation races
  are flagged without spinning, one notification/file only, bounded output and
  NEW-path refusal. Existing eight tests and all legacy scene/count/pixel oracles
  remain intact. A short-deadline test is not a native timing result.
- After scoped tests/build/source review, there may be at most ONE separately
  admitted native progress capture, only if independent review agrees it answers
  the stated stalled-stage question. Retain a unique binary, fresh lane quiet
  acknowledgements, both mutexes and the external 210s guard. No automatic native
  launch follows this spec approval. Do not retry after timeout or passing pairs.
  If no stall reproduces, report the question unanswered. Any stall receipt is
  phase evidence, not a backend root cause. Renderer changes, historical parity
  closure and T20 cadence still require separate causal review and original gates.

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
