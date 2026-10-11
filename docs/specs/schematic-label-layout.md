# Bounded schematic symbol label layout

Ticket T-0122, parent S-041. Initial stack boundary is the immutable T114 code/proof head `95faf8245a65d05cb2ef0c01ef38cdb3e374e020`; only T122 commits will later be integrated on the authorized merged base. This contract precedes Rust changes and requires independent approval.

## Public sources and observed problem

Read 2026-10-11:

- [KiCad 9 schematic editor: symbol properties and fields](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#symbol-properties) documents independent field placement and automatic placement. Its symbol editing documentation distinguishes pin connection endpoints, names and numbers. These public behaviors motivate readable presentation; no KiCad implementation, assets or library data are used.
- [egui 0.36.2 Painter](https://docs.rs/egui/0.36.2/egui/struct.Painter.html) documents screen-point painting, clip bounds, pre-layout measurement and returned text rectangles. Logical UI points can differ from physical framebuffer pixels.
- [SVG 2 text positioning](https://www.w3.org/TR/SVG2/text.html#TextLayout) defines positioned text and anchor/baseline behavior. Existing Viamind exports outline bundled Hack glyphs independently.

The original T114 native proof shows qualified package references and values centered over their body and endpoint text. All five units are present, but labels overlap. Native and SVG currently choose independent offsets. T114 proof remains unchanged and accurately discloses the gap.

## Scope and preservation

Provide one deterministic, bounded presentation plan for placed schematic symbol reference/unit, value and pin number/name text. It covers ordinary single-unit symbols and explicit complete multi-unit/power members, every existing cardinal rotation and local mirror, odd-Nm dimensions and interior/coincident/dense pin endpoints.

No project/schema/history/command change is required. Preserve every stored symbol/body/pin/wire coordinate, identity, electrical type, package membership, reference/value string, connection endpoint and existing body/stem geometry. Preserve selection/picking, cross-probe rings, wire attachment and connectivity. Presentation leaders and text boxes never become electrical wires, selectable model objects or snapping anchors. Explicit net labels/junctions remain at user-authored locations; this is not automatic net-label relocation.

Existing human Fit/pan/zoom controls and existing agent SVG/PNG tools expose the presentation. No human-only design action or new inferred model field is introduced. Full session, document status, native bytes/path and event cursor remain unchanged during layout, camera adjustment, hover and in-memory rendering.

## Bounded borrowed admission

Reuse the existing schematic view bounds before creating a label plan: at most 10000 selected-sheet structural objects including package membership, 100000 actual pins, 4096 UTF-8 bytes per original string, 256 KiB selected-sheet known text and 32768 visible source glyphs. Charge counts, package rows and original string/glyph budgets before copying/truncating text, constructing per-pin layout records or sorting. Generated qualification punctuation and abbreviation suffixes also count toward a separate 32768 displayed-glyph budget before constructing display strings. Do not make a whole-project snapshot. Borrowed sheet admission must run before any new secondary label-plan allocation; the existing independently bounded native sheet snapshot may be used as its input.

Invalid identity/unsupported transform/arithmetic overflow or exceeded label bounds produces an actionable view diagnostic. It must not silently turn an unavailable view into a complete successful drawing. In-memory render errors preserve the existing output/no-publication behavior. Already decoded transport allocation remains outside this secondary presentation claim.

Per-symbol stacking is at most O(p log p), sheet work is bounded by admitted objects/pins/text, and record count is linear. There is no global all-label pair search, unbounded retry placement or background task queue. Shared arithmetic uses checked integer Nm/i128 intermediates; model values are never converted and stored back from floats. UI/raster scaling happens only at the final rendering boundary.

## Shared label plan

Put the reusable plan in the render crate, which already depends on doc/geom and is consumed by native UI and existing render tools. It contains stable original object IDs/label roles, full source text, bounded display text, deterministic world anchors/envelopes and optional graphical leader segments. No engine-to-render dependency is added.

Use horizontally upright labels after transforming the actual body corners and actual endpoints. The world body envelope and all endpoint extrema determine protected geometry. Pin labels have deterministic outside-body lanes. Choose a lane from the pin's local body face/direction with a documented deterministic tie for interior/equidistant pins, then transform its direction to the world side; no electrical direction is inferred. Sort each world lane by its along-edge endpoint coordinate and pin UUID, and advance overlapping text envelopes by a fixed positive row spacing. Place world-left/right lanes first, on opposite sides of the body. Place world-top/bottom lanes outside the complete vertical envelope of body/endpoints and left/right label boxes, then pack along X. This keeps orthogonal lane text boxes disjoint even at corners; reference/value rows are outside all pin-label boxes. A leader connects a displaced label to its own unchanged endpoint; use a visually distinct thin/dashed presentation style. It does not change the existing electrical pin stem or ring.

Reference/unit text is above the protected body/pin/label envelope; nonempty value is below it. Preserve the exact qualified reference format `U1:A (Amplifier A)` for explicit package units. Package lookup is indexed once after bounded membership validation. Native and SVG/PNG use the same label roles, displayed strings, upright world-side choice, lane order, anchors and conservative envelopes. Existing stable export text IDs remain associated with the same original UUIDs.

Initial metrics: one millimetre nominal text height, 0.7-em advance envelope per displayed Unicode scalar, 1.6-mm line envelope/stack spacing and at least 0.5-mm geometry/text clearance. Use the unchanged bundled Hack monospace family where supported. Checked envelopes must contain the actual native measured glyph rectangles and exported glyph bounds at the verified zooms; increase conservative padding if a tested glyph needs it. Text anchor/baseline conversion is explicit in each output adapter. Native and SVG rasterization need not produce identical pixels.

Reference/value/pin display text is limited to 32 Unicode scalars plus an explicit ASCII `...` suffix when abbreviated. Count original text first; truncate only on UTF-8 boundaries. No silent field loss: native hover exposes exact full source text and role/UUID, while render metadata retains exact full source text plus displayed text and abbreviation flag. Existing model queries/properties retain exact full strings. This is display abbreviation, not model normalization, identity inference or a full Unicode grapheme shaping claim. Unsupported export glyph behavior stays explicit as in existing schematic exports.

## Fit and native zoom detail

Fit bounds include the conservative label/leader envelopes along with original drawing geometry, so fitting cannot place a label outside the viewport merely because its text was omitted from old geometry-only bounds. Camera changes remain local, keep the full design/session unchanged and preserve existing pointer/world anchoring semantics.

Native font size follows the one-mm world size at useful zoom, with a maximum of 32 logical points. Below ten logical points nominal size, symbol reference/value/pin labels are omitted together and a visible canvas notice says to zoom in to read schematic labels. Geometry and exact picking remain active; small illegible text is not represented as readable. Reappearing detail uses the identical retained deterministic plan, without moving the document or changing selection. The notice is view-local and is not an electrical exclusion or ERC result.

SVG/PNG exports include all admitted planned labels at their world size regardless of native interactive detail suppression. SVG remains zoomable; a small whole-sheet PNG can have small labels. Metadata describes this deliberate detail policy and any abbreviations. At equal view scale the native/export shared anchors and strings agree; fonts/DPI/rasterization and native size cap are explicitly outside pixel equivalence. No claim is made that arbitrary dense/long or overlapping components can all be legible in one minimum-size whole-sheet view.

## Required tests and actual proof

Before implementation, independently review the public contract, preservation boundary, count/text precedence, metrics, detail and export policy.

Meaningful tests must cover:

- Independent expected odd-Nm body corners and endpoint coordinates under all cardinal/mirror poses; exact originals/IDs/session/history/status unchanged after layout/fit/hover/export.
- Actual measured label rectangles outside their own body/pin geometry, reference/value separation and deterministic stacked dense/coincident pin rows. A leader retains exact pin UUID/endpoint ownership and is excluded from picking/attachment.
- Single-unit, explicit A–D plus P power fixture, long/abbreviated UTF-8 strings, stable ordering across repeated layouts and maximum-plus-one/malformed poison admission before new allocations. Exact full text remains in tool metadata/hover and serialized project.
- Native/SVG/PNG shared semantic label anchors, displayed text and leaders; correct deliberate low-zoom policy, and visible zoom-in notice with geometry/picking preserved.
- Actual egui Fit/pan/wheel and body/pin picking on original complete five-unit/fourteen-pin fixture at a useful wide fit, plus compact 640x400 inspection of one unit and long/dense pin labels. Check effective ancestor clip and every relevant label rectangle, not only widget existence.
- Actual retained production two-client in-memory rendering with full SessionSnapshot/document status/native-byte equality. Existing render file guards, modes and tool catalog behavior remain intact.
- Self-closing actual native WGPU wide/compact framebuffer captures using production UI, exact source/binary/PID/viewport/scale/alpha provenance and lossless original RGBA/RGB readbacks. Native evidence must visibly improve the original crowding, not just move overlapped text outside the capture.
- Four Linux gates, focused geometry/render/input checks, actual catalog drift/workflow verification and final exact-head CI. No latency target is claimed without an actual measured report.

## Explicit exclusions

This is bounded local symbol-label presentation, not a global schematic autorouter or collision-free page typography solver. Independent overlapping symbols/net labels/wires can still overlap labels; users retain explicit placement and zoom/pan. There is no custom symbol art, editable persisted field pose, arbitrary-angle text/body transform, new hidden pin behavior or full schematic-equivalence claim. Detached library editor previews are not a second label-placement editor and retain their existing exact definition controls.
