# Existing copper property editing verification

The independently reviewed [public spec](specs/existing-copper-property-edits.md) adds exact explicit property changes for one existing straight track or circular through via. Stable object IDs and stored endpoints/center remain unchanged. No implicit sizing, movement, net inference, bulk edit or repair is performed.

## Shared guarded commands

`query_copper_properties` captures only the selected original object, bounded document policy, project UUID and complete source cursor. `set_track_properties` changes width/top-or-bottom/net; `set_via_properties` changes outer diameter/drill/net. Both commands require the exact original object and initial full cursor. Unknown keys and oversized raw net strings refuse before secondary copies. Named policy, board structural/text and positive replacement-net growth budgets are charged while borrowed before one geometry-only board clone; unrelated assembly and schematic data stay excluded and unchanged.

The shared production checker evaluates the complete private candidate. An edited-object violation or incomplete/truncated result refuses atomically. Document width is exact; drill/annular/clearance floors are the maximum of existing request baselines and document minima. Existing unrelated violations and inherited checker exclusions may remain. This is scoped checking, not physical certification. Equal/no-op changes refuse. Each successful edit produces one stable-ID delta, one design event and one undo entry.

## Actual desktop inputs

Five actual egui tests exercise pointer-selecting a track, opening the Board menu, exact local odd-Nm edits, layer/net changes, scene dimensions, one undo/redo, copper-row via editing, invalid precision/geometry refusal, persistent 640x400 controls, Escape/Cancel/modal guards, peer edits and same-UUID reopen. Field UUID identities remain stable. Stale Apply is disabled while local strings remain; explicit Reload discards strings and captures the current original. Apply refreshes from the returned committed delta and captured cursor, avoiding an unguarded peer-state recapture.

## Verification provenance

Final combined source and binary hashes, full Linux gate counts, actual two-client production desktop HTTP results, measured small-fixture commit latency and self-closing native WGPU readbacks are listed in the evidence master. This ticket's commits were transplanted alone onto the reviewed literal T110 head. The full gate runs on that combined source, with a mutex through doctests/builds/unique copies and the canonical shared target.

Native readbacks are inspection-only hosts running production ViamindApp and renderer. Genuine egui text-edit events prepare local strings, complete session/status invariants are checked before and after every UI pass, captures use exclusive create/fsync and the host closes after success. Logical focus retention occurs only inside the inspection host. UI passes do not measure display cadence. There is no OS-input or compositor-screenshot claim. Existing production/preview windows remain preserved.

The strict 100k-board 60fps criterion remains open. Final exact-head CI, independent review and prospective-main-plus-HEAD tree equality gate merge.

## Gate and runtime evidence

The [evidence master](verification/t0111/master.json) indexes source/binary provenance and durable artifacts. Historical combined source `a83d8b8` passed fmt, strict workspace all-target Clippy, **811 workspace tests, zero failures, one existing ignored test**, five additional actual UI checks and Linux workspace build. Discovery exposed a schema mismatch: runtime already required explicit nullable `net`, while generated schemas allowed omission. Source `2512b2a` corrects that contract and adds the schema/runtime regression; **24 focused checks**, strict lint, fmt and workspace build passed. Actual canonical **96-tool** discovery, **18 workflow checks** and **13 tooling tests** passed against that exact corrected application.

The actual production desktop binary passes **45 two-client HTTP assertions** and a separate read-only headless process passes **seven checks**. Assertions cover exact odd-Nm fields, stable IDs/endpoints/center, document floors, joint collisions/holes, null nets, strict schemas, stale sequence/stream/original objects, undo/redo and conservative guide changes. Small-fixture HTTP commits measured **4.21–5.95 ms** round trip under current load; this does not establish a large-board latency bound.

Native inspection has separate source/binary provenance. The inspection host requests an opaque viewport and uses child-only XWayland environment variables for background frame callbacks. It preserves every original Screenshot-event Color32 RGBA channel in an exclusive/fsynced P7 file, records alpha minimum/maximum and nonopaque pixel count, and saves identical RGB samples in P6. Review PNGs deliberately omit alpha; every RGB sample and all original RGBA bytes are verified by [verify-artifacts.py](verification/t0111/verify-artifacts.py). No claim is made about final compositor opacity. Raw captures stay at the retained paths indexed in the master; review PNGs and runtime reports are committed.

Earlier compact Wayland capture stalled without a readback; only that owned failed host was terminated. An initial X11 host incorrectly assumed stored alpha must equal 255 and failed before writing its capture. Both attempts remain excluded and their logs are retained. Production application behavior was unchanged by the inspection correction.

A subsequent inspection attempt refused capture when prepared absolute kittest control coordinates did not fit the negotiated native X11 viewport. A second attempt refused a size match because native logical font/pixel-grid dimensions differed. The final host obtains the exact safe widget IDs through actual labeled button focus in preparation, then reads their actual native responses and requires all three enabled footer buttons inside the actual window/content. No geometry tolerance is used. Captured physical pixel dimensions are reported independently of requested logical viewport dimensions and native DPI. This host-only correction changes no property or application behavior.

The root reviewer independently repeated all **45 actual two-client HTTP checks** against the same production binary. Its report is retained separately, with its own external fixture/native-path provenance and hash in the master; the verifier did not modify repository or user files.

The exact-ID host reached Screenshot successfully but refused its initial four-million-pixel buffer limit on the actual high-DPI X11 framebuffer. The final inspection-only capacity is explicitly bounded at 16M pixels/64MiB original RGBA; logical viewport requests and actual physical captures are reported separately. This capacity correction preserves the same strict native geometry/session assertions and exact channel verification.

At packaging the original production process/listener was absent. Available coredump/kernel/journal checks found no matching crash or OOM record; its exit cause is undetermined. The same exact retained binary was restarted on the verified free owned port, with separate launch/PID/live hash provenance. Four fresh two-observer read-only checks verify exact track/via/full cursor and unchanged complete session/status. The two 45-check mutation reports remain historical evidence of their respective runs.

## Readable native views

The [track editor](verification/t0111/track-wide-capture.png), [via editor](verification/t0111/via-wide-capture.png) and [compact via editor](verification/t0111/via-compact-capture.png) were inspected independently. Wide captures are 2667×1667 physical pixels; compact is 1067×667. Requested logical viewports are 1600×1000 and 640×400. Original RGBA, alpha omission in review PNGs, full-session invariants and actual footer bounds are documented separately in the master.
