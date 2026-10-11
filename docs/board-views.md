# Read-only SVG and PNG board views

Export without opening a desktop or connecting to a display:

```sh
viamind --export-png --open board.viamind.json --output new-board.png
viamind --export-svg --open board.viamind.json --output new-board.svg
viamind --export-svg --open board.viamind.json --output top-region.svg \
  --view-layers top_copper,outline --view-bounds 10,5,40,25 --view-size 1200,800
```

The destination must be a new file in an existing directory. Existing files,
directories and symlinks are preserved; there is no overwrite mode. The active
native project cannot be the destination. A sibling temporary file is written
and synced before atomic publication. An interrupted process may leave a
`.viamind-svg-` or `.viamind-png-` staging file; atomic visibility does not guarantee power-loss
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
to 4096 UTF-8 bytes per string and 2 MiB aggregate board text, and SVG output to
16 MiB. Primitive/text budgets are checked before cloning a board-only snapshot;
unrelated schematic content is not cloned or validated. Omitted legacy part
fields count toward the board snapshot budgets. Half-nanometre pad edges and via radii
are retained at the SVG boundary. Project/object descriptions are escaped XML;
valid Unicode remains intact and XML-forbidden characters are replaced with
U+FFFD and reported. Views contain no external resources or executable content.

Legacy graphical footprint previews are omitted and counted. No pads are
inferred from a footprint name. Schematics, inner copper and pours are excluded.
This is an inspection view, without electrical or fabrication certification.
Board PNG and SVG are supported; schematic views remain separate work.

## PNG rendering limits

PNG uses the same geometry builder with a private crop-relative coordinate origin;
response regions retain absolute model coordinates. This preserves small features
near distant model origins. Extreme zooms reject if intersecting geometry bounds
exceed a conservative 1/8-pixel float precision budget; zoom out or hide the large
layer. Geometry is rasterized by pinned resvg with text/fonts and external image
loading disabled. PNG is static RGBA8 with transparent background, holes, cutouts
and aspect margins. PNG has an additional 4194304-pixel area limit and an 8 MiB
encoded byte limit. Same inputs are deterministic within the pinned build;
anti-aliasing can differ between renderer versions.

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

`render_board_png` accepts the same input and returns one MCP `image` content
block (`mimeType: image/png`, base64 data), alongside structured `media_type`,
`metadata`, `source_cursor` and optional `path`. Image data is not duplicated in
structured/text metadata. The optional destination has the same new-file
protections. Each live registry permits two concurrent PNG renders; snapshots
and cursor guards are evaluated after acquiring a render slot. A cancelled
client retains its slot until its blocking render has actually ended.

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
pixels. The PNG product exporter can also be compared against this independent raster.
See the public specifications for [SVG](specs/svg-board-views.md) and
[PNG](specs/png-board-views.md).

Product PNG verification uses Pillow for independent decoding and librsvg for an
independent raster comparison of uniform interiors, rotated/mirrored pad samples,
physical holes, track caps, layer isolation, cropping and aspect margins. Rotation
and opaque-hole negative controls prove that the checks can detect those errors:

```sh
python scripts/verify-board-png.py /absolute/path/to/viamind \
  fixtures/views/asymmetric.viamind.json /tmp/new-png-verification
```
