# Scoped read-only SVG board views

Behavior specified before implementation for T-0057. Public primary sources:

- [W3C SVG 2 coordinates](https://www.w3.org/TR/SVG2/coords.html), sections 8.4–8.7: SVG begins with X right/Y down; viewBox and preserveAspectRatio map the requested region into a pixel viewport without changing model geometry.
- [W3C SVG 2 painting](https://www.w3.org/TR/SVG2/painting.html): round stroke caps, even-odd polygon filling, and fills/strokes.
- [W3C CSS Masking Module Level 1](https://www.w3.org/TR/css-masking-1/), sections 9.1–9.2: clipping and user-space masks can represent through holes without painting opaque fake drill centers.
- [W3C XML 1.0](https://www.w3.org/TR/xml/), sections 2.2, 2.4 and 2.11: valid Unicode characters, character/entity escaping.

## Scope and access

Render an immutable PCB snapshot as deterministic self-contained SVG. This increment supports explicit rectangular SMD pads, straight top/bottom copper tracks, circular through vias and explicit polygon outline/cutouts. It is a board inspection view, not manufacturing output or connectivity/clearance certification. Legacy graphical footprint previews, schematic views, inner copper and pours are excluded and reported. Do not invent electrical geometry from a footprint name. Full S-030 PNG acceptance remains open.

Expose one renderer through a headless CLI export and MCP snapshot tool. MCP accepts an optional exact expected_cursor and returns the actual source cursor, SVG media type/data and view metadata; optional file output uses the same bytes. Export never changes document, selection, history or native dirty state. A desktop action is outside this initial increment.

## Geometry, layers and region

Retain model origin and X right/Y down, matching the desktop board orientation. Use integer nanometres until the SVG I/O boundary. Place pad centers using the shared Part local-X mirror/CCW rotation/translation; centered rectangular pads use SVG rotation equal to negative stored CCW degrees. Compare oriented corners independently. Tracks have exact stored width and round caps. Via copper appears on both outer copper layers; through-drill openings punch through rendered content using a shared mask. No whole-bottom-layer mirroring.

Supported layer filters: top_copper, bottom_copper, outline. Deduplicate and use stable drawing order regardless of request order. Outline fill uses even-odd outer/cutout paths with visible round-join contour stroke; no copper inferred from it. Through holes remain physical openings across selected layers. Preserve aspect ratio and clip to the requested region. Return metadata identifying units, axes, selected layers, region and omitted unsupported previews.

The region is either explicit integer-nm min/max bounds or an automatic outward-rounded fit to supported selected geometry including physical widths, plus explicit nonnegative padding (default 1 mm). An empty view uses a declared 10 mm region centered at model origin. Explicit bounds must have positive width/height; do not silently expand a requested region. The viewport has configurable integer pixel width/height (default 1600×1000, each 1–4096). Bound coordinates to ±100000 mm, padding to 1000 mm, supported primitives to 100000 and output to 16 MiB. Reject invalid model geometry, nonfinite CLI conversion or excessive bounds/counts before filesystem publication.

## Text and publication

Include escaped project/object descriptions and stable IDs for semantic inspection. Escape XML markup delimiters in text/attributes; valid Unicode text round-trips. Encode carriage returns as character references so XML line-ending normalization preserves them. Replace XML-forbidden code points with U+FFFD and report that replacement in metadata; never emit scripts, event attributes, foreign objects, external resource URLs or model-derived element names. Bound individual metadata strings to 4096 UTF-8 bytes and fail oversized text explicitly.

Optional output is a NEW file; there is no overwrite mode. Protect the active native path and existing files/directories/symlinks. Write/sync a sibling temporary file, then publish atomically without replacing a destination. Parent directory must exist. No partial final output on validation/write/collision failure; interrupted processes may leave identifiable staging files. Atomic visibility does not promise power-loss durability.

## Verification

Parse generated XML and independently render synthetic SVG with a compatible open-source viewer. Check asymmetric rotated bottom-pad oriented corners, exact track caps/widths, drilled via transparency including overlaid tracks, closed outer/cutout geometry, layer filtering and explicit clipping/auto-fit bounds. Test Unicode/quotes/markup/control text, no active content/resource injection, invalid and bounded requests, collision/native protection, actual headless CLI and MCP cursor/state preservation. All required Linux fmt/clippy/workspace test/build gates pass before submission; parent PNG acceptance stays open.
