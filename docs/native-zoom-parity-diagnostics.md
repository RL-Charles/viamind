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

The physical window matches the original 1400x1000 at (2180,1265). Both used the
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
100k/60fps acceptance remain open. Further Rust/native work requires the next
public progress/watchdog amendment and independent review.
