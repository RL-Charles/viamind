# Scoped PNG board snapshots

Behavior specified before implementation for T-0060. Public primary sources:

- [W3C PNG Third Edition](https://www.w3.org/TR/png-3/), sections 4.4–4.5, 11.2 and 13.16: lossless raster representation, truecolor with alpha, dimensions and transparent/opaque sample interpretation.
- [W3C SVG2 coordinates](https://www.w3.org/TR/SVG2/coords.html), sections 8.4–8.7, and [CSS Masking](https://www.w3.org/TR/css-masking-1/), sections 9.1–9.2: preserve the shared SVG viewBox, aspect ratio, clipping and physical hole mask semantics.
- [MCP tools specification, 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), Image Content and Structured Content: return base64 PNG in an actual image content block with `mimeType: image/png`, alongside structured view/source metadata.
- [resvg 0.47 API](https://docs.rs/resvg/0.47.0/resvg/), [features](https://docs.rs/crate/resvg/0.47.0/features), [usvg processing options](https://docs.rs/usvg/0.47.0/usvg/struct.Options.html), and [tiny-skia Pixmap API](https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Pixmap.html): parse an internally generated static SVG tree and render it into a bounded transparent pixmap, then encode PNG.

## Scope

Rasterize the supported board SVG snapshot from T-0057 without a desktop, browser, GPU or external process. Reuse that renderer's geometry, layer filters, region, pixel viewport, automatic fit, axes, support limits, text validation and exclusions. Do not add a second geometry implementation. Support explicit rectangular SMD pads, straight outer tracks, through vias, polygon outer/cutout geometry. Physical holes/cutouts and aspect-ratio margins remain transparent. Output static 8-bit RGBA PNG with exact requested pixel dimensions. No animation, HDR, arbitrary SVG input, visible font rendering or external resource loading.

The result is a board inspection image, not electrical/fabrication certification. Legacy previews remain omitted and counted; schematics, inner copper and pours remain excluded. Board PNG capability advances S-030; its acceptance for any schematic view remains open until separately implemented and verified. No desktop action in this increment.

## Access and bounds

Expose display-free CLI `--export-png --open PATH --output NEW_FILE`, sharing the `--view-*` options and mutually exclusive invocation validation with SVG. Add MCP `render_board_png`, using the same immutable snapshot, optional exact expected_cursor, optional new-file path and metadata as SVG. Return actual MCP image content and structured media/geometry/source/path metadata, without duplicating base64 inside structured JSON. Ordinary JSON-only tools retain their existing response behavior; image response style is registered by trusted tool code rather than inferred from model-controlled JSON fields.

Keep existing per-dimension limits of 1–4096, additionally cap PNG area to 4194304 pixels and encoded PNG to 8 MiB. Use checked allocation/size calculations and explicit errors before publication. Limit concurrent PNG rendering per live tool registry to two jobs; capture/check the document snapshot after obtaining a permit. CLI performs one render. Rendering cannot change document, selection, history, events or dirty state. Later document edits are absent from an already captured view.

Render only SVG produced by the shared bounded model renderer, never caller-provided markup. Disable renderer text, system-font, font-memory-map and raster-image features. Deny external image resolution explicitly and leave resource/font databases unloaded. Background remains transparent, with no guessed board geometry or opaque fake drill centers. Same inputs produce deterministic bytes within the pinned dependency build; pixel rasterization may differ across library versions/platforms.

Publish optional output through shared bounded sibling staging/new-file publication, protecting existing files/directories/symlinks and the active native file. There is no overwrite mode. Parent directory must exist. Validation, render, encode and collision failures leave no partial final file. Interrupted processes may leave identifiable staging files; no power-loss durability guarantee.

## Dependency evaluation and license gate

Candidate: pinned resvg 0.47.0, default features disabled. Its [published manifest](https://docs.rs/crate/resvg/0.47.0/source/Cargo.toml) declares Apache-2.0 OR MIT and Rust 1.87; the [release MIT license](https://raw.githubusercontent.com/linebender/resvg/v0.47.0/LICENSE-MIT) is compatible with Viamind's MIT distribution. It uses usvg 0.47 and tiny-skia 0.12, without native rendering libraries. [tiny-skia's published manifest](https://docs.rs/crate/tiny-skia/0.12.0/source/Cargo.toml) declares BSD-3-Clause and a PNG feature. PNG encoding uses the already present MIT OR Apache-2.0 png crate. Verify the resolved minimal dependency closure and actual packaged license files before adding production dependencies; retain required notices through the Linux bundle license machinery. No proprietary code, assets or undocumented binary behavior is involved.

## Verification

Decode real generated PNG with an independent implementation and verify signature/IHDR dimensions, RGBA transparency and supported pixel colors. Compare independent librsvg rendering of the shared SVG at geometry interiors; tolerate documented anti-aliasing differences at edges. Exercise asymmetric rotated/mirrored pad placement, cap/width geometry, through-hole transparency over underlying copper/substrate, cutout/exterior transparency, filtered layers, explicit region/aspect ratio and empty views. Negative controls should distinguish the wrong rotation sign and opaque hole centers.

Test invalid/oversized requests before publication, collision/native/symlink and atomic failure behavior, deterministic output, exact read-only cursor/state preservation, stale requests and fresh unsaved geometry. Verify actual CLI without display and actual MCP transport returns decodable image content plus compact structured metadata, without base64 duplication. Run all Linux fmt/strict workspace all-target clippy/workspace tests/workspace build gates and package license validation before submission.
