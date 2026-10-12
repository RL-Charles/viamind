# Native populated-zoom parity investigation

T-0126 is in progress. The historical failure is unresolved; no renderer correction
or 100k performance result is claimed.

## Preserved failure

The unchanged release on merged source `e7f60cf489ddb673ad416f991fb86c398c61a2ed`
failed only populated 8x case 4: 581 drawn objects, mean channel difference
6.939124503968254, maximum 215, and 346026/1068480 pixels differing by more than 2.
Its first four cases had maximum differences 1, 1, 1, 2 and no pixels over 2.
The 100k cadence example never launched. Original binaries, source, reports,
quiet admission and system receipts remain indexed in
[the original baseline](verification/t0126/original-baseline/baseline-blocked.json).

The retained original inventory contains launch-time window geometry, system
configuration, executable provenance and comparison metrics. It has no original
RGBA image, per-case camera/PPP/root dimensions, input journal or frame/pass
receipts. The launcher aborts at the parity assertion before persisting its
in-memory start/completion summary. Launch geometry alone does not establish
case-4 geometry throughout the run. Source-level auto-fit and untagged screenshot
hazards do not establish that either occurred in that historical run.

## Initial bounded diagnostic

Diagnostic-only source `ce135b238667993f5bf215254389fd71958d4635` preserves the
legacy case transitions, scene, renderer and criteria. Its unique executable
SHA256 is `4a78ff55e5eef121cda4e9d36d0d429bb12af2ac7e22758b23832fba9e2b236f`.
Scoped formatting, strict release example lint, four meaningful diagnostic tests
and the release example build passed. This is not a full workspace gate receipt.

One quiet native run on 2026-10-11 23:44 UTC self-closed PID 3504424 after 20.05s.
It retained ten uniquely identified original RGBA images (56MB total), bounded
typed camera/frame/cache receipts and no unexpected screenshots. All five pairs
passed the original criteria and the historical stricter max<=2/zero-over-2
observation. The first four results exactly match the failed run. Case 4 instead
drew 606 objects, maximum difference 1, mean 0.0001792265648397724 and zero pixels
over 2. That changed visible count prevents treating this as a fix.

The physical window matches the original launch metadata: 1400x1000 at (2180,1265). Both used the
1920x1200/60.002Hz monitor at scale 1 and default display environment. Actual trace
DPI is 1; canvas is [280,23,1400,977], matching the original 1068480 compared pixels.
Case 4's pair holds camera center [33.8096233769221,25.375284784012628] and zoom
53.69680915111379 through request/draw/arrival. The added via and quad remain in
the scene but are offscreen in that case. Original case-4 camera/input/pass
receipts do not exist, so their equality to this trace cannot be established.

All ten original P7 RGBA files and P6 RGB derivatives are retained with lossless
gzip. Every RGBA and RGB sample was independently compared to its review PNG;
all actual alpha samples are 255. RGB reviews omit alpha explicitly. The master
reports metadata admission before final report serialization; streamed final
serialization still enforces the same aggregate 1MiB budget. Image arrival-frame
receipts establish stable drawing state, not a physical scanout claim.

[The initial master](verification/t0126/initial-trace/master.json) indexes hashes,
raw receipts, source/binary provenance, scripts, scope, reports and all ten pairs.
Run `python docs/verification/t0126/initial-trace/verify.py` with ImageMagick to
verify all indexed hashes and every original PNG sample.

## Next diagnostic gate

Independent root review approved public amendment `96e10124483456624180bdf7b633264837f2f2e0`
before further Rust: bounded actual-app headless layout investigation followed
by at most one deferred-I/O comparison. Ten precharged shared image handles may
retain at most 640MiB of unique pixels; image/JSON I/O moves after the terminal
arrival or abort. Fixed borrowed navigation counters exclude text. Original
transitions, geometry and whole-canvas criteria remain unchanged.

Programmatic example camera edits leave startup auto-fit active, so resize is a
possible state hazard to test. That is not historical causality. Cross-stage raw
comparisons also reject the simple hypothesis that the prior fitted image was
used for case 4: those comparisons differ by mean 53.79 and 1059737 pixels over 2,
unlike the original failure. A repeated passing deferred comparison will still
leave the historical cause unresolved. No additional native retry, renderer
change, visual-correctness closure or cadence follows automatically.

## Deferred comparison: incomplete

Approved diagnostic source `9783c974b83f42938408e9f0d4b85761665eabd5`
passed formatting, strict release example lint, eight example tests and the
release example build. The headless host uses the actual unwrapped egui root:
all five viewport/camera/count observations match the initial native trace. A
controlled one-pixel resize overrides direct camera navigation while auto-fit
remains active. This demonstrates a risk, not the historical cause.

Exactly one quiet deferred-I/O comparison launched the unique executable
`7183877eb4d1bf1914b59ad7be34f983ba02b973856dfea135998f5bd6a4d287`
as PID 3567262. Its physical window again matched 1400x1000 at (2180,1265).
Unlike the approximately 20s initial run, this attempt remained alive beyond the
210s external watchdog. The created parity report stayed empty; no terminal
trace or deferred pixel files were flushed. Its stage and parity are unknown.

The fresh exact-window screenshot shows zoom 6.712 and 4624/5002 drawn objects,
consistent with case-3 geometry. It does not expose the internal step or pending
request. Main-thread `wchan` was epoll wait; readable worker wait locations were
retained. Kernel thread stacks and one non-elevated debugger attachment were
refused, so no call stack or backend cause was obtained. After executable hash
and process start time were revalidated, only this owned helper received SIGTERM;
its absence was verified. No replacement launch followed.

Pinned egui-wgpu source unmaps the capture buffer before sending an owned CPU
`ColorImage`; the event subsequently shares that image through `Arc`. Retaining
the image does not itself retain that mapped GPU buffer. The example's 180s guard
executes inside UI callbacks, so it cannot stop an event loop that stops invoking
them. These source observations do not establish why the native helper stalled.

[The incomplete-attempt master](verification/t0126/deferred-incomplete/master.json)
indexes 21 artifacts, including scoped successes and earlier test-host failures,
quiet admission, exact executable provenance, timeout, owned-window screenshot,
refused stack evidence and bounded retirement. Original failure and initial
passing pairs remain unchanged. Historical causality, visual closure and strict
100k/60fps acceptance remain open. The independently approved progress/watchdog amendment and its single run follow
below. The Cua route name describes the capture driver fallback; it does not
establish which backend the application used.


## Independently timed progress run: complete, historical cause unresolved

Public amendment `e51311ad` was independently approved before Rust. Diagnostic
source `43600a009e110dec52d8b4aa462e5273827d8da2` adds an optional independent
180-second observer with thirteen fixed atomic fields, generation consistency
and one bounded terminal notification. It adds no GUI locks, extra screenshot
requests, redraws or sequence image writes. Its fixed state, channel/path,
requested thread stack and streamed 8KiB output allowance are precharged inside
the same 1MiB metadata budget. The worker joins after the native loop returns.

Scoped formatting, strict release example lint, all fourteen example tests and
the release example build passed. All eight prior tests remain, including the
exact five-state headless camera/count observations and controlled one-pixel
resize risk. Six new tests cover independent timeout, active/changing callback
observations, one terminal signal, sender disconnect/NEW output, streaming cap
and metadata reservation before worker creation. This is not a full workspace
gate receipt. The unique executable SHA256 is
`89a361806cfc67813f663f732a2526ab72f4a7283d7289cabfda9cb74e5d25e7`.

Exactly one newly admitted quiet run launched PID 3747131 at 2026-10-12
00:49 UTC and self-closed in 8.58 seconds. The independent receipt completed
through terminal notification after 8.32 seconds: 81 callback entries and exits,
coherent generation 162, ten requests prepared/submitted/arrived, no pending
sequence and the returned callback phase. No timeout or stalled stage occurred,
so this run does not answer where the earlier helper stalled.

Ten original RGBA images (56MB) and complete request/draw/arrival receipts were
flushed at the terminal stage. All paired provenance checks passed. All ten
arrival camera/viewport/DPI/layer states and all five comparison metrics exactly
match the initial passing diagnostic. Case 4 again drew 606 objects with maximum
difference 1, mean 0.0001792265648397724 and zero pixels over 2; the historical
failure drew 581. Full screenshots include changing UI statistics, so equality
of the reported canvas metrics does not assert equality of all full-window
pixels. The native window remains 1400x1000 at (2180,1265), DPI 1.

[The progress master](verification/t0126/progress-trace/master.json) indexes
73 artifacts, twenty compressed original/derived raw files, exact executable
provenance, scoped gate, quiet admission, completion/atomic receipts, all ten
pairs, derived difference maps and the initial-versus-progress comparison.
Run `python docs/verification/t0126/progress-trace/verify.py` with ImageMagick to
rehash every artifact and compare every original RGBA/RGB PNG sample. Actual
alpha is 255; RGB review explicitly omits it. No scanout or OS-input proof is
claimed.

The historical image mismatch and the single deferred stall remain unresolved.
No renderer behavior changed, and the 100k cadence example has never launched
in this investigation. Further native experiments or corrections require a
specific evidence-based scope and independent review; repeated passing runs
are not visual-correctness closure or a strict 60fps result.

## Optional hardened harness: validated, historical cause unresolved

Public amendment `c912fc9ac04910ed061aef7926356f865831332e` was independently
approved before Rust. `Canvas::set_manual_camera` validates finite coordinates
and the existing zoom range before changing anything, then installs the exact
camera and disables startup auto-fit. Existing `focus_world` behavior remains.
The optional `--hardened` diagnostic requires deferred trace I/O and the progress
watchdog; ordinary and legacy trace modes remain available.

The harness counts six completed integration frames rather than UI passes,
refuses known discarded passes before screenshot submission, and requires exact
tokens, root viewport, dimensions, camera, DPI, layers and intervening draw state.
Wheel/pinch/Home/middle-button/Space-drag contamination aborts with fixed numeric
evidence and no private text. Expected root pixels come from the unrounded full
`InputState` viewport and effective PPP, matching pinned egui-winit's window
inner-size input and egui-wgpu's surface texture. Safe-area content bounds and
UI-rounded bounds are unsuitable. Ambiguous pixel recovery refuses before a
request; equal-but-wrong image dimensions retain raw evidence and refuse.

Source `c0e2ccd3db319a5b8dd1b1900c3c2891a0097e05` passed formatting, release UI
all-target strict Clippy, all 22 example tests and the release example build.
Four unchanged Canvas tests, including exact/invalid manual-camera and retained
focus semantics, passed at `ff180e3`. The examples include actual root five-case
camera/count preservation across resize, discarded-pass admission, privacy,
both-direction dimension mismatch, matching-but-wrong dimensions, fractional
DPI/safe areas and metadata ownership limits. These are scoped receipts, not
a full workspace gate. The uniquely retained executable SHA256 is
`660e5de117a60afd86c8cf9ceb1e58a27890230edf275a6b899dbcac59cde399`.

Exactly one fresh quiet native validation launched PID 4006395 on 2026-10-12
01:22 UTC. It self-closed after 9.89 seconds, flushed ten original images and
released both shared mutexes. The atomic observer received terminal notification
after 9.59 seconds with 92 callback entries/exits, ten arrivals and no pending
request. The 91 retained drawing passes represent 90 distinct observed integration
frame numbers; neither count is a display scanout or performance measurement.

All five whole-canvas comparisons passed, including max differences 1/1/1/2/1
and zero pixels over 2. Camera/count/viewport/DPI/layer observations exactly match
the previous passing progress run after removing the new root-pixel field. The
physical window again was 1400x1000 at (2180,1265), PPP 1; every actual image and
requested root size is 1400x1000. Each unique request had six completed stable
frames and three matching retained draws before arrival. Independent raw-sample
analysis reproduced every metric and verified every RGBA/RGB PNG sample. Actual
alpha is 255; RGB review images explicitly omit it.

[The hardened master](verification/t0126/hardened-trace/master.json) indexes
the scoped gates, exact executable, quiet/system/atomic receipts, all ten pairs,
independent admission audit and pixel analysis. Run
`python docs/verification/t0126/hardened-trace/verify.py` with ImageMagick to
verify all indexed hashes and original PNG samples. This validates the optional
hardened harness on these cases. It does not reconstruct the original case-4
camera or explain why 581 objects were drawn then versus 606 now. The historical
image mismatch and deferred stall remain unresolved; no renderer change, new
native retry, 100k cadence result or strict 60fps acceptance follows from it.

## Integrated workspace gates

The first full combined run on rebased source `f4320d2f` failed honestly: the
four workspace gates passed (1033 tests, 0 failed, 1 ignored), but 9 of 22
`gpu_parity` example tests failed in the debug profile on epaint's debug-only
assertion for dropped, unapplied texture deltas. Earlier scoped runs were release
builds, so the assertion never fired. Test-only source `570ba277` clears the delta
in one headless helper, matching existing headless tests.

The rerun on `570ba277` passed formatting, strict workspace all-target Clippy,
test-cfg example Clippy, the workspace tests, all 22 debug example tests, a
normal workspace build immediately after retouching every Rust input, the exact
120-tool generated reference and 18 isolated agent workflow checks. The retained
debug executable SHA256 is
`85ffa048ce4a6db0e71bac99cd6ee7781350b94ac7a3a8828be6618d761f3a87`.
[The combined-gate master](verification/t0126/combined-gate/master.json) indexes
both runs; `python docs/verification/t0126/combined-gate/verify.py` checks it.

This increment ships the bounded optional diagnostics and the validated
`Canvas::set_manual_camera` API. It does not identify the historical case-4 cause,
change the renderer, or provide a 100k cadence result; those remain open.
