# Polygon board outlines and cutouts

Public behavior reference: the [KiCad PCB editor manual, Board outlines](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)
describes closed outlines and enclosed, non-intersecting cutouts. Viamind independently defines the
polygon model and workflows below. Rendering uses the ISC-licensed
[earcutr triangulation API](https://docs.rs/earcutr/0.5.0/earcutr/fn.earcut.html).
No external EDA implementation code is used.

## Model and persistence

- Board has optional outline: an implicitly closed outer polygon and zero or more internal
  cutout polygons. Vertices are integer nanometres. Legacy designs without an outline keep the
  existing preview rectangle; explicit outlines remain fixed when parts move outside them.
- Require 3 or more vertices per ring, nonzero area, distinct adjacent endpoints and simple rings.
  Adjacent edges may share only their common endpoint; nonadjacent edges cannot touch/cross/overlap.
  Cutouts must be strictly inside the outer boundary, disjoint and non-nested. Either winding is
  accepted. Bound outline coordinates to +/-10000 mm and total vertices to 2048, cutouts to 64.
  Enforce these invariants on native deserialization as well as command input.
- Outline-bearing files use native version 2 so version-1 applications reject them explicitly.
  Files without outlines retain version 1 and its byte-identical deterministic representation.
  Loading an outline in a version-1 envelope rejects rather than silently accepting mislabeled data.
  Event logs with an outline-bearing base also use version 2; outline-free bases keep version 1.
  New outline delta variants cause older log readers to reject them explicitly.

## Shared action

- `set_board_outline` takes `outline` (outer/cutouts with vertices in mm), or null to restore the
  legacy preview. Optional `expected` contains project_id and the exact current model outline
  (integer nanometres, null when absent). Mismatch rejects atomically. Desktop always supplies it.
- Validate before changing anything; emit one board-outline delta, undo/redo and event replay
  restore exact geometry, no-op preserves history. Outline changes participate in dirty state.
- Desktop Board Outline form has multiline outer vertices and cutouts, each line X,Y in mm,
  blank lines separating cutouts. Apply uses the catalog action; Cancel changes nothing.
  A stale form remains reviewable; reopening reads the latest geometry.
- Draw the actual substrate, including concavity and empty cutouts, and all boundary edges.
  Camera fit includes the outline and any objects outside it. Polygon geometry is triangulated
  when the scene changes, rather than on every frame.

Arcs, slots and DXF outline import follow as separate increments of the parent story.
