# Create a PCB component from a schematic symbol

Open a native schematic project, then choose **Synchronize… → Create PCB
component from symbol…**. This workflow creates one component and its electrical
pads from an explicitly chosen native footprint.

1. Refresh symbols and choose an unlinked symbol.
2. Enter a native `viamind-footprint` version1 JSON library path. Original examples
   are `fixtures/footprints/original-smd2.viamind-footprint.json` and
   `fixtures/footprints/original-smd3.viamind-footprint.json`. Their numbered pads
   match the capacitors and regulator in `fixtures/schematic/regulator.viamind.json`;
   these examples carry no manufacturing certification.
3. Enter the origin in integer nanometres (1000000 nm = 1 mm), board side and
   quarter-turn rotation. For example, X 40000000 / Y 30000000 places at 40 / 30 mm.
4. Prepare and inspect the reference, value, footprint, placement and pad nets.
5. Create the component. One Undo removes the component and all its pads; Redo
   restores their exact identities. Refresh to create another component.

The footprint's unique pad numbers must exactly match the symbol's unique pin
numbers. A symbol's explicit footprint assignment must match the chosen library
name. A missing assignment allows an explicit choice; creation leaves the
schematic unchanged. Duplicate board references and existing symbol links reject.

Pad nets use the same exact native graph identities as forward synchronization.
Two sheet-local labels with the same name remain separate unless the native graph
connects them. New components can attach to existing named copper without
changing that copper. Run DRC to inspect actual placement and clearances after
creation. This action does not certify connectivity, routing or fabrication.

Concurrent edits and document replacement invalidate the preview. Refresh and
prepare again; an old preview cannot silently create a component in a changed
project. File loading and graph preparation run in one background worker, which
continues safely if the dialog is closed.

## Agent tools

`preview_schematic_pcb_component` takes `expected_project`, current `source_cursor`
and `placement` (`symbol_id`, detached `footprint`, integer `position`, `side`,
`rotation_mdeg`). Obtain the project and cursor with `get_session_state`.
The preview is read-only and returns properties, numbered net assignments and
an exact `command`. Pass that command unchanged to `create_schematic_pcb_component`.
The shared catalog command recomputes and validates before one atomic edit.

The result is bounded to synchronization's 10000 model objects, 4096 bytes per
string, 4 MiB aggregate text and 8 MiB serialized guard, plus existing native graph
limits. Geometry is rectangular SMD with quarter-turn transforms; transformed pad
corners and origins must stay within ±10000 mm. Footprint files retain their 1 MiB
and 1024-pad limits. There is no automatic footprint search, component deletion,
bulk replacement, routed-net migration or hierarchy support in this action.
