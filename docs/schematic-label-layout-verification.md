# Schematic label layout verification

T-0122 implements the independently approved [public behavior contract](specs/schematic-label-layout.md). One bounded integer layout supplies native text and SVG/PNG label anchors. Model coordinates, UUIDs, electrical endpoints, picking anchors and shared engine state remain unchanged.

## Implemented behavior

References, explicit unit qualifiers and values sit outside the complete body/pin envelope. Pin number/name labels occupy deterministic outside lanes; dashed leaders connect displaced labels to the original exact endpoint. Leaders and label hover regions have no electrical, snap or pick meaning. The four cardinal poses and mirror transformation use the same integer geometry as the original symbol.

Each display string uses at most 32 Unicode scalars followed by an explicit ASCII `...`; hover and export metadata retain the complete original text. The native canvas suppresses labels below 10 logical points with a visible zoom notice and caps the font at 32 points. Static SVG/PNG retains the selected labels at the declared world font size. These are explicit detail policies, not pixel/font/DPI equivalence.

Native default-font typographic and ink bounds are checked before painting against the common envelope. An oversized glyph produces an actionable object/role diagnostic while geometry and picking remain available. Export validates the complete selected source text, including a tail hidden by abbreviation. Disabling Symbols or Pins also excludes the corresponding text and leaders before glyph validation.

## Original Linux gates and actual input

Production source `2c8417bb3fde054436afac777f8d202e6d08b10a`, retained through `verification/t0122/native-source`, passed fmt, strict workspace all-target Clippy, all workspace tests (**885 passed, zero failed, one existing ignored**), workspace build and the native inspection-host build. The full shell subsequently exited 1 because its catalog command mistakenly used `docs/reference`. Separate corrected Python commands passed the canonical `docs/generated` comparison for **109 discovered tools**, **18 isolated workflow checks**, and the independent Pillow/librsvg render oracle (**57,628 uniform interior samples**). This path correction changed no Rust code.

Focused input tests verify Fit, responsive refit when the existing sidebar changes width, wheel zoom, exact center selection, presentation-only nonselection, cancellation and editor mode changes. A 640×400 case fits eight coincident explicit pins and a 32-scalar value at 10 logical points. Low zoom displays the detail notice. All retain complete SessionSnapshot and DocumentStatus. Independent render tests exercise odd-nanometre corner sets in all eight cardinal/mirror poses, 128 dense pin lanes, deterministic ordering, bounds before malformed geometry, generated abbreviation budgets and selected-layer isolation. Actual native font tests cover supported non-ASCII and fallback glyph containment/refusal.

The retained logs include early test/setup failures and their scoped corrections: pinned egui test entry, test texture cleanup, responsive-sidebar fit oracle, catalog directory typo, missing Python Pillow environment, static-oracle net-label index and a whitespace-invalid verifier fixture. No failed run is represented as successful.

## Actual production MCP

The exact retained production executable passed **31 stdio checks and 31 checks over two actual desktop HTTP clients**. The committed [driver](../scripts/verify-schematic-labels-native.py) requires an explicitly owned scratch engine; its setup opens original fixtures and deliberately creates a dirty board selection before read-only render assertions. It compares complete source cursors, graph metadata and SessionSnapshot/DocumentStatus across successful and refused exports.

The checks cover five unit-qualified references, fourteen original pin endpoints, 24 unique owned labels, independent exact pose transforms, disjoint owned envelopes, outlined SVG, independently decoded transparent PNG, SVG/PNG plan equality, isolated Symbols/Pins exports, stale cursor refusal, NEW output/no-clobber/native-path refusal, full-source unsupported-glyph tails and long ASCII abbreviation with retained complete metadata. Latencies are recorded for observation; no performance acceptance threshold is claimed.

## Native WGPU evidence

The inspection-only example uses the production ViamindApp and actual native eframe WGPU Screenshot event. It checks full session and document status before and after every UI pass, measures actual default-font text/ink against the shared envelopes, and checks visibility and disjoint text rectangles on these original fixtures. It writes synchronized NEW original P7 RGBA and P6 RGB files, then closes itself. The review PNG preserves every original RGB sample. Original alpha remains separately retained; no opaque-alpha claim is made.

The wide original five-unit capture demonstrates reference/value/pin readability at useful Fit in all four rotations and mirrors. Compact captures show eight dense pins and a long value at inspection zoom, plus explicit low-zoom detail suppression. Launch metadata records native pixel dimensions separately from requested logical dimensions. Preparation uses egui test input; **no native OS input is claimed**. The compact host refresh only enters manual camera mode using an actual canvas Zoom event before transfer into a different-DPI native viewport; product source remains unchanged.

## Combined integration

Only this ticket's eleven commits were transplanted onto literal T-0117 head `d4a4baa1be4e304e40c1001dc1e231e555292d98`; every range-diff entry is equal and there were no conflicts or manual feature edits. Combined source `6157a2b647a63e0c305d289145c2d687bae932ba` passed fmt, strict workspace all-target Clippy, **43 focused render/native-font/actual UI/wire/view/MCP tests**, workspace and inspection builds, 13 tooling tests, the exact **113-tool** catalog, 18 workflow checks, 31 stdio and 31 two-client actual desktop HTTP checks. The independent static reader again passed all 25 assertions and 57,628 interior samples. The original full-workspace proof is retained separately; no redundant second full test run is claimed.

Three fresh native captures verify the integrated agent status bar alongside unchanged label geometry: [five-unit Fit](verification/t0122/combined-wide.png), [compact dense inspection](verification/t0122/combined-compact.png), and [low-zoom notice](verification/t0122/combined-low.png). Every original RGBA/RGB sample and metadata remains retained. The compact canvas labels and new agent controls are readable; inherited schematic sidebar controls still extend below the minimum viewport, visible in the images. This ticket does not change that sidebar layout. All six historical/final inspection processes closed themselves after capture.

Reproduce the actual transport checks against an explicitly owned scratch process with `uv run --no-project --with pillow python scripts/verify-schematic-labels-native.py --binary /absolute/viamind --repo /absolute/checkout --report /NEW/report.json`; add `--url http://127.0.0.1:PORT/mcp` for two actual HTTP clients. Its setup replaces only that scratch project's state. The original source fixtures are checked unchanged. Exact-head CI and independent review are the remaining merge gates, tracked in Sherpa/PR separately from the completed local proof.

## Limits

This is bounded per-symbol layout, not a global page collision solver. Separate symbols, wires, junctions and net labels can still overlap when authored that way. Unicode scalar abbreviation does not promise grapheme shaping. Unsupported native fallback bounds and unsupported selected export glyphs refuse explicitly. Source admission remains 10,000 schematic records, 100,000 pins, 4,096 bytes per text, 256 KiB aggregate text and 32,768 source/generated glyphs. No design/history/native-file changes are introduced by labels or inspection.

The [master](verification/t0122/master.json) indexes exact source refs, executable hashes, gate logs, actual transport reports, original framebuffer provenance and review images. Run `python3 docs/verification/t0122/verify-artifacts.py --retained` on this host to check durable hashes, exact PNG readback, original RGBA/RGB and retained executable/process identities. Final combined integration and exact-head CI are recorded separately from original proof.
