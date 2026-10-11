# Detached native footprint definitions

Choose **Libraries → Footprint definitions…**. A selected PCB part is unnecessary.
Enter a name, add blank rectangular pads, then enter each pad's number, local X/Y
and width/height in decimal millimetres. Preview the local geometry and choose
**Save NEW footprint definition** with an explicit unused path in an existing
directory. **Load native footprint definition** edits a saved native library as
a detached draft. It leaves the source file unchanged.

Coordinates retain exact nanometres (at most six decimal places in mm). Local X
points right and local Y points down. Width and height must be positive; pad
numbers must be unique, trimmed and nonempty. Geometry is bounded to ±10000 mm,
dimensions to 10000 mm and definitions to 1024 rectangular SMD pads. The preview
marks the local origin and labels pads when the display rectangle is large enough.
A named empty definition is valid. Blank newly added rows need explicit entry.

Closing the window retains its draft. Reset explicitly clears the local name and
pad rows. During file work, row/name/path edits and Reset are disabled; closing
retains the one pending operation. Errors retain your fields. Peer project edits
and document replacement retain this independent local draft.

## Reuse on a component

Saving a definition changes no placed component, selection, history, event cursor
or project dirty state. Select the target component, choose **Edit electrical
pads…**, enter the saved path under **Library file path**, and choose **Replace
pads from library**. This separate exact project/part guarded edit is undoable.
Matching pad numbers retain pad UUIDs and nets; new numbers receive fresh UUIDs
and unassigned nets. Component placement, rotation, side, linked symbol and
assembly metadata remain intact. A stale target snapshot rejects the whole edit.

The native library stores only its name and numbered local rectangular geometry.
It stores no object identities, nets, board side, orientation, population or
sourcing fields. Courtyard, silkscreen, mask, paste, holes and other pad shapes are
outside this editor's scope. No centroid, mounting or manufacturing rules are
inferred; a preview is not a fabrication approval.

## Shared agent actions

`load_footprint_definition` takes `{path}` and reads strict `viamind-footprint`
version 1 JSON. `save_footprint_definition` takes `{footprint, path}` and publishes
one NEW native file. Files are limited to 1 MiB and paths to 4096 encoded bytes.
Both use the same shared actions as the desktop. Existing destinations and
symlinks refuse publication; the current native project path remains protected
under the file lifecycle guard. Native file loading is bounded and nonblocking
for nonregular files. Unknown schema fields reject.

`import_footprint` remains the separate guarded component application action.
