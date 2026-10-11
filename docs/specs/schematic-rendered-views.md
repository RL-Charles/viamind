# Bounded selected-sheet SVG and PNG views

Behavior specified before implementation for T-0068. Public primary sources:

- [W3C SVG2 coordinates](https://www.w3.org/TR/SVG2/coords.html), sections 8.4–8.7: viewport mapping, uniform aspect-ratio fitting and clipping.
- [W3C SVG2 text](https://www.w3.org/TR/SVG2/text.html), sections 11.1–11.3 and 11.10: glyph outlines and metrics, text placement and anchoring. Outlining a packaged font makes images independent of installed fonts.
- [W3C PNG Third Edition](https://www.w3.org/TR/png-3/): bounded static truecolor with alpha output.
- [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools): actual image content with structured source/view metadata.
- [resvg 0.48.1 text feature](https://raw.githubusercontent.com/linebender/resvg/v0.48.1/crates/usvg/Cargo.toml), [usvg Options](https://docs.rs/usvg/0.48.1/usvg/struct.Options.html) and [WriteOptions](https://docs.rs/usvg/0.48.1/usvg/struct.WriteOptions.html): load an explicit in-memory font database, convert generated text to paths, and disable system fonts, memory-mapped fonts, SVGZ and raster images.
- [Packaged Hack font bytes](https://raw.githubusercontent.com/emilk/egui/0.36.2/crates/epaint_default_fonts/src/lib.rs) and [font notices](https://raw.githubusercontent.com/emilk/egui/0.36.2/crates/epaint_default_fonts/fonts/Hack-Regular.txt): reuse the unchanged Hack Regular already shipped with the desktop, retaining MIT, public-domain DejaVu and Bitstream Vera notices. Evaluate the complete resolved feature/license closure before production dependencies change.

## Scope and geometry

Render one explicitly selected native schematic sheet. Existing model fields are the only source: rectangular symbol bodies, exact local-X mirror followed by 0–3 CCW quarter turns in X-right/Y-down coordinates, pin endpoints and body-clamped pin stems, straight wires, explicit junctions and attached labels. Do not infer connections from crossings or display implied junctions. Do not modify model, UI selection, dirty state, undo/redo or event history. Hierarchy, custom symbol artwork, electrical-rule certification and board geometry are excluded.

Use the existing integer model transforms for body corners and pin stems/endpoints, including asymmetric odd-nanometre dimensions. Inspection styling has fixed physical sizes: 0.15 mm round wire/body strokes, 0.10 mm pin stems, 0.15 mm endpoint rings, 0.20 mm junction radius and 1 mm text em. References and values are upright around the body center; pin numbers/names and scoped label names are upright beside their world anchors. This is a schematic inspection view with physical text sizing, rather than the desktop's screen-pixel text styling. Preserve full semantic text and stable IDs in escaped SVG metadata as visible text is outlined. Unsupported glyphs fail with an explicit code point; never silently drop a label. No font fallback, remote font or system font lookup.

Output self-contained SVG with a pixel-coordinate viewBox and transparent margins, uniformly fitted and centered. Response metadata records the absolute model-space nanometre region, X-right/Y-down axes, pixel viewport, fixed font, counts and exclusions. Translate geometry relative to the crop before the f32 rendering boundary; reject views whose intersecting geometry cannot meet a conservative 1/8-pixel precision budget. Use one shared outlined SVG geometry for PNG; no separate schematic drawing implementation. PNG is static RGBA8. Deterministic bytes are promised within the pinned build, not across future library versions/platforms.

Layer filters select wires, symbols (body/reference/value), pins (stems/endpoints/numbers/names), junctions and labels. Deduplicate and paint in that stable order. Automatic fit includes selected geometry and conservative visible text extents plus padding. Empty selected views fit a 10 mm square. Explicit regions clip; never expand them silently.

## Access, bounds and publication

Add display-free CLI `--export-schematic-svg` / `--export-schematic-png`, required `--sheet UUID`, `--open PATH`, `--output NEW_FILE`, existing region/size/padding flags and a separate `--schematic-layers` list. Board layer flags cannot select schematic layers. Add MCP `render_schematic_view` and `render_schematic_png`, with required sheet ID, options, optional exact expected_cursor and optional NEW path. SVG returns text plus structured metadata; PNG returns one real image/png content block without duplicate base64 in structured metadata. The source cursor is captured atomically with the selected sheet and active native path after waiting for a render permit.

Before cloning, count selected-sheet objects (10000), pins (100000), aggregate selected-sheet text (256 KiB), each metadata string (4096 UTF-8 bytes), and visible glyphs (32768). Validate selected sheet local invariants; do not traverse, clone or validate unrelated board/sheets. Capture only project ID/name and the selected sheet. Direct renderer APIs accept this bounded snapshot type, avoiding whole-project cloning. Legacy sheets with no explicit content yield an empty view with a reported exclusion.

Viewport dimensions remain 1–4096; SVG and PNG area both at most 4194304 pixels. SVG maximum 16 MiB, PNG maximum 8 MiB. Limit generated shape/path work and text before parsing/allocation; reject oversized output before publication. Two concurrent schematic render jobs per registry, permit held until blocking work completes even after client cancellation. No arbitrary SVG input or external resource resolution.

Reuse atomic sibling staging/new-file publication and native-file protection. Existing files, symlinks and directories survive collisions; no overwrite flag. Validation/render/collision failure leaves no partial final file. Parent must exist. Interrupted processes may leave named staging files; no power-loss durability guarantee.

## Verification

Evaluate the pinned font and minimal text feature closure in a disposable program before adding dependencies. Verify engineering Unicode, detect unsupported glyphs, outlined font-free SVG, deterministic PNG, and no system/external resources. Retain font notices in the actual Linux bundle license machinery.

Use asymmetric rotated/mirrored symbols with explicit pins, wires, junctions and Unicode/escaped labels. Independently render outlined SVG with librsvg and decode PNG with Pillow, check orientation/endpoints/text visibility/transparency/crop/layer filtering, and reject wrong-transform and missing-text controls. Test absent/legacy sheets, malformed selected-sheet identities, invalid/over-budget requests, unrelated giant invalid board/sheets excluded before cloning, deterministic bytes, protected collisions, exact cursor/state preservation and fresh unsaved edits. Exercise production CLI and actual MCP transports, then all Linux fmt/strict clippy/workspace test/build gates and combined CI.
