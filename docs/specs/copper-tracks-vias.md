# Straight copper tracks and through vias

Public behavioral sources consulted before implementation:

- [KiCad public PCB editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html):
  straight track geometry, layer/width/net properties, via diameter and drill;
  through vias span the outer copper layers while other via types have distinct
  layer restrictions.
- [KiCad public board format documentation](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/):
  start/end/width/layer/net attributes for segments and position/size/drill for vias.

This is independent behavior/model design; no existing implementation is copied.

## Supported model

Board maps store tracks and through vias by globally unique UUID. Empty maps are
omitted so legacy outline/pad-free files retain their original bytes. Coordinates
and lengths are integer nanometres. Track: distinct endpoints, positive width,
top or bottom copper layer, optional named net. Through via: position, positive
outer diameter and drill smaller than diameter, optional named net. Via copper
spans both supported outer layers. No inferred inner stack, blind/buried/microvia,
curved track, solder-mask relief or fabrication readiness is represented.

Bounds: coordinates within ±10000 mm; lengths at most 10000 mm; at most 100000
tracks and 100000 vias per board. Net names are absent or trimmed, nonempty,
control-free Unicode strings at most 128 characters. Native loading validates
geometry, map key/identity agreement, global identity collisions and counts.
Native schema version 4 is required only when nonempty copper maps are present;
older builds must reject before silently dropping copper. Event logs use version
4, with lower-version copper deltas/base snapshots rejected.

## Commands and desktop

`add_track`, `delete_track`, `add_via`, `delete_via` are typed shared commands,
automatically available over MCP. Inputs use finite mm at the boundary, rounded
to integer nm and validated again so sub-nanometre widths cannot become zero.
Optional expected project IDs guard document replacement; optional exact expected
objects guard deletions. Every successful create/delete is one undo step/event;
validation failures leave project, log and history unchanged. Tracks/vias are
independent objects with net labels, not references to a component or pad.
Deleting a part does not delete independent copper or establish a new short.

Desktop Board → Copper opens an accessible coordinate form for adding a segment
or through via and a list to remove existing copper with guarded commands. Fields
are mm and stored top-view coordinates (Y down). Concurrent edits keep a failed
form open with an error. Native rendering uses the actual track width with round
caps and the via's annular ring/drill opening; camera fitting includes copper.
Layer visibility respects top/bottom copper and vias controls.

## Connectivity boundary

Named pad groups and their straight-line guide estimates remain pad-only. This
increment does not infer copper continuity, remove guides, route automatically,
check shorts/clearances or produce manufacturing files. Those checks require
subsequent connectivity and DRC work.
