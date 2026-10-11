# Read-only schematic SVG and PNG views

Select a sheet UUID from `query_schematic` or the native project, then export without a display:

```sh
viamind --export-schematic-svg --sheet SHEET_UUID --open design.viamind.json --output new-sheet.svg
viamind --export-schematic-png --sheet SHEET_UUID --open design.viamind.json --output new-sheet.png \
  --schematic-layers symbols,pins,wires,junctions,labels \
  --view-bounds 0,0,30,20 --view-size 900,600
```

`--sheet` is required. Use `--schematic-layers` for schematic categories; `--view-layers` selects board copper and cannot be combined with schematic export. The five schematic categories default to all and repeated entries deduplicate in drawing order: wires, symbols, pins, junctions, labels. Shared bounds/size/padding flags use the same units as board views. Without explicit bounds, fit selected geometry and conservative text extents with 1 mm padding (`--view-padding-mm`, 0–1000). Empty views use a 10 mm square. Explicit bounds clip without padding; aspect ratio is preserved with transparent margins.

## Geometry and visible text

The renderer shows existing explicit native rectangular symbol bodies, mirrored/quarter-turned pin endpoints and body-clamped stems, straight wires, junction dots and scoped labels. Local X is reflected before CCW quarter turns in X-right/Y-down coordinates. Odd-nanometre body extents match the native integer transform. It never invents junctions at wire crossings or infers custom artwork from a symbol name.

References, values, pin numbers/names and labels are upright. Fixed physical inspection styling uses 1 mm text em, 0.15 mm body/wire strokes, 0.10 mm pin stems, 0.15 mm endpoint rings and 0.20 mm junction dots. The native desktop uses screen-pixel text sizes; inspection image text scales with the model-space crop.

Visible glyphs use unchanged bundled Hack Regular and are converted to paths. No viewer/system font, remote font or external image is needed. Common engineering text such as `10kΩ µA é Я` is supported. An unsupported visible character fails with its Unicode code point; there is no silent missing-glyph fallback. Text in categories explicitly omitted by the layer filter need not have font coverage. Original text and stable source IDs are retained in escaped SVG metadata, alongside the font notices. Outlined glyphs are not selectable text.

SVG has a pixel-coordinate `viewBox` (0,0,width,height). Metadata declares the exact absolute model-space `region_nm`, centered uniform mapping and axes; crop-relative coordinates are used before the float render boundary. This differs from board SVG's absolute model-space viewBox. PNG rasterizes the exact outlined SVG and is static RGBA8. Transparent background/margins remain transparent. Same inputs produce deterministic bytes within the pinned build; anti-aliasing may differ in future versions/platforms.

## Agent tools and limits

`render_schematic_view` returns SVG text with media type, metadata, source cursor and optional published path. `render_schematic_png` returns one actual MCP `image/png` block and structured metadata without duplicated image base64. Both desktop HTTP MCP and headless stdio expose them:

```json
{"sheet":"SHEET_UUID","options":{"layers":["symbols","pins","wires"],"region_nm":{"min_x":0,"min_y":0,"max_x":30000000,"max_y":20000000},"width_px":900,"height_px":600},"path":"new-sheet.png"}
```

Metadata object counts describe the complete captured sheet; category filters and clipping select the drawing and do not change those source counts.

Only `sheet` is required. Optional exact `expected_cursor` guards against stale snapshots. Capture checks budgets before cloning and copies only project id/name and the selected sheet. Unrelated board/other sheets are not copied or validated. Views preserve model, selection, dirty state and undo/history. Edits after capture are absent from that image. Two schematic jobs per live registry share a permit pool for SVG and PNG; cancellation retains the permit until blocking work ends.

Limits: 10000 selected-sheet objects, 100000 pins, 256 KiB aggregate selected-sheet text, 4096 UTF-8 bytes per string, 32768 visible glyphs, viewport dimensions 1–4096 and area at most 4194304 pixels, SVG 16 MiB, PNG 8 MiB. Extreme crops fail if intersecting geometry exceeds a conservative 1/8-pixel float precision budget. Widen the region or hide the large category. Legacy sheets without explicit content yield an empty view with `legacy_empty_sheet: true`.

Output files must be NEW, with an existing parent. Existing files/directories/symlinks and the active native file are protected; no overwrite mode. Validation/render/collision failures leave no partial final file. Publication uses synced sibling staging and atomic no-clobber creation; interruption can leave identifiable staging files, and power-loss durability is not promised.

Hierarchy, custom symbol artwork, other sheets, board geometry and electrical certification are excluded. Rendering displays explicit geometry and text; use electrical tools separately for connectivity/ERC.

## Independent verification

With Pillow and librsvg (`rsvg-convert`):

```sh
python scripts/verify-schematic-view.py /absolute/path/to/viamind \
  fixtures/views/schematic.viamind.json /tmp/new-schematic-verification
```

The reader independently parses outlined SVG/semantic text, rasterizes with librsvg and decodes the product PNG with Pillow. It checks asymmetric mirrored quarter-turn pins/body, junctions/wires, labels, transparency, layer isolation, explicit crops and aspect margins. Wrong-transform and removed-text controls must fail the intended checks. See the [public behavior specification](specs/schematic-rendered-views.md) and [font notices](licenses/Hack-Regular.txt).
