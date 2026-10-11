# Read-only SVG board views

Export without opening a desktop or connecting to a display:

```sh
viamind --export-svg --open board.viamind.json --output new-board.svg
viamind --export-svg --open board.viamind.json --output top-region.svg \
  --view-layers top_copper,outline --view-bounds 10,5,40,25 --view-size 1200,800
```

The destination must be a new file in an existing directory. Existing files,
directories and symlinks are preserved; there is no overwrite mode. The active
native project cannot be the destination. A sibling temporary file is written
and synced before atomic publication. An interrupted process may leave a
`.viamind-svg-` staging file; atomic visibility does not guarantee power-loss
durability.

## Geometry and options

Views retain the model origin and X-right/Y-down orientation, matching the
board desktop. Both sides are viewed from above; bottom copper is not globally
mirrored. Explicit rectangular SMD pads use the shared component mirror/rotation
transform. Tracks have their stored width and round caps. Through vias appear on
both copper layers with transparent physical drill holes, including across
underlying tracks and substrate. Polygon outlines use an even-odd fill for
cutouts. Drawing order is substrate, bottom copper, top copper.

Supported `--view-layers` entries are `top_copper`, `bottom_copper`, `outline`.
The default selects all; repeated entries are deduplicated. `--view-bounds`
takes `min_x,min_y,max_x,max_y` in model millimetres, with positive extents.
Without explicit bounds, the view fits selected supported geometry plus
`--view-padding-mm` (default 1, range 0–1000). Empty views use a 10 mm square
centered on the origin. `--view-size` sets the pixel viewport (default
1600,1000; each dimension 1–4096). Aspect ratio is preserved, so mismatched
region/viewport proportions leave transparent margins. Requested regions clip
geometry; explicit bounds never receive automatic padding.

Coordinates are bounded to ±100000 mm, primitives to 100000, metadata strings
to 4096 UTF-8 bytes, and output to 16 MiB. Half-nanometre pad edges and via radii
are retained at the SVG boundary. Project/object descriptions are escaped XML;
valid Unicode remains intact and XML-forbidden characters are replaced with
U+FFFD and reported. Views contain no external resources or executable content.

Legacy graphical footprint previews are omitted and counted. No pads are
inferred from a footprint name. Schematics, inner copper and pours are excluded.
This is an inspection view, without electrical or fabrication certification.
This increment exports SVG only; product PNG view support remains open.

## Shared agent snapshot

`render_board_view` uses the same renderer in desktop HTTP and headless stdio MCP:

```json
{"options":{"layers":["top_copper","outline"],"region_nm":{"min_x":10000000,"min_y":5000000,"max_x":40000000,"max_y":25000000},"width_px":1200,"height_px":800},"path":"new-board.svg"}
```

`options` and `path` are optional. Omit `path` to return only SVG data. Optional
`expected_cursor` is the exact cursor from `get_session_state`; stale requests
reject before rendering or publication. The response includes `media_type`,
`svg`, `metadata`, `source_cursor` and the optional absolute `path`. Metadata
identifies axes, units, region, layers, supported geometry counts, omitted legacy
parts and XML text replacement. Document edits after snapshot capture are absent
from that view. Rendering preserves the document, selection, event/history and
dirty state.

## Independent reader check

With librsvg (`rsvg-convert`, tested 2.62.3) and Python Pillow installed:

```sh
viamind --export-svg --open fixtures/views/asymmetric.viamind.json \
  --output /tmp/board-view.svg --view-bounds 0,0,60,40 --view-size 1200,800
python scripts/verify-board-view.py fixtures/views/asymmetric.viamind.json /tmp/board-view.svg --binary /absolute/path/to/viamind
```

The script parses XML independently, compares oriented asymmetric pad corners,
track endpoints/width/caps and closed cutout paths, then renders a verification
PNG with librsvg and samples drill transparency, annuli and substrate/cutout
pixels. This external verification PNG is not a product PNG export feature.
See the [public behavior specification](specs/svg-board-views.md).
