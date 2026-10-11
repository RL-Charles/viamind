# Detached native schematic symbol templates

## Public behavior source

The [KiCad 9 Schematic Editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols)
describes reusable symbols with body geometry, pin definitions and fields, and
explicit import/export between library files and schematic instances. Viamind
independently implements its smaller native rectangular, single-unit subset
below. No external code, symbols or artwork are used.

## Detached data

1. A strict `viamind-symbol` version 1 JSON envelope contains one named template:
   rectangular width/height, default value, optional explicit footprint name,
   and ordered pins with number, name, local integer-Nm position and native
   electrical type. Unknown fields at every level reject.
2. No project, sheet, instance or pin UUID, reference, net, wire, position,
   instance rotation/mirror or PCB link is stored. Export copies committed
   definitions into a detached template; it never changes the schematic.
3. Files are regular files of at most 1 MiB; bounded reads check the opened file
   again. At most 1024 pins, 4096 UTF-8 bytes per field and 256 KiB aggregate
   inspected text are accepted before copying or serialization. Template names
   and pin numbers are nonempty/trimmed/control-free; pin names can be empty.
   Pin numbers are unique. Existing native field length limits also apply. Native
   dimensions and local pin coordinates obey existing ±10000 mm limits.
4. Export atomically publishes a NEW file in an existing directory. Existing
   files, the captured native document path and partial publication are
   protected. Source cursor guards are full stream UUID plus sequence.

## Guarded placement

5. Import/placement creates a fresh symbol and fresh pin UUIDs. The caller supplies
   an explicit reference, position, quarter-turn rotation and mirror flag; the
   template supplies body/pins/default fields. Neither existing symbols nor any
   linked PCB component are updated. Source library bytes are unchanged.
6. Capture a bounded target sheet with project UUID and full source cursor.
   Commit requires that exact sheet/project/cursor, validates resulting symbol
   geometry and unique nonempty reference across the project, then applies one
   existing sheet transaction. All visited global validation identities/text are
   bounded before inherited validation allocates. Invalid/stale/oversized input
   rejects without partial edits; one undo removes the entire instance and redo
   retains its identities.
7. MCP and desktop use the same template validation and guarded placement command.
   File I/O, parsing and export staging stay in a retained background worker in
   the desktop. A hidden window retains its worker; duplicate starts reject.
   Changed/reopened document or target-sheet context cannot apply an old preview.
8. The desktop exposes explicit source/destination paths, template export name,
   target reference and transform, a detached load preview and an explicit place
   action. Controls and result/error messages remain reachable at 640×400.
   Model/history/selection stay unchanged while loading, exporting or navigating.

## Verification and exclusions

Use original templates and existing original regulator geometry. Verify strict
file/field/pin/text limits, detached export, fresh/replayed identities, exact
transformed pins, library byte preservation, atomic undo/redo, stale/reopened
guards, duplicate references, concurrent edits, new-file/native protection,
desktop input/parity, actual production MCP and readable native captures.

This foundation does not claim multi-unit packages, arbitrary vector graphics,
live library links, updating existing instances, datasheet extraction, hierarchy,
third-party library import or new project schema support. The parent multi-unit
acceptance remains open.
