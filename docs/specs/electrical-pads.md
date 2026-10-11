# Electrical pads and named connectivity

Public behavioural reference: [KiCad 9 PCB editor, working with pads and pad properties](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#working-with-pads). Pads have local position, size, number and electrical net membership; footprint placement transforms pad geometry into board coordinates. This independent implementation uses no source code from that application.

## Initial supported behaviour

- A part may carry explicit rectangular surface-mount pads, each with a stable UUID, unique nonempty number, local integer-nanometre centre and positive width/height. An optional trimmed net name identifies electrical membership across parts. Unassigned pads have no net.
- Absent pads preserve the legacy built-in graphical preview; an explicitly empty pad list means no electrical pads. Preview pads never imply connectivity.
- `set_part_pads` replaces one part's pads atomically, guarded by its exact expected Part snapshot. Input uses millimetres, existing pad IDs are retained by number when omitted, new pads receive new IDs. Supplied IDs must belong to that part. Numbers/IDs must be unique, net and number lengths bounded, 1024 pads per part maximum and coordinates/dimensions bounded to 10 metres. Failed validation leaves model/history unchanged.
- Part deltas own pads, so deletion, movement, undo/redo, selective undo conflicts and replay keep geometry/membership together. Native loading rejects malformed or duplicate pad identities, mismatched keys and IDs colliding with other objects.
- Native projects containing explicit pads use version 3; legacy outline-free files retain version 1 bytes, outline-only files version 2. New event logs use version 3 so appended Part snapshots containing pads cannot be silently discarded by older readers.
- Desktop selected-part editor exposes one row per pad with number, local X/Y, width/height and net, plus Add/Remove/Apply/Reload. Drafts reject concurrent edits. Copper and mask use true rotated rectangles, mirrored for bottom placement; mask currently expands by the existing 0.05 mm preview allowance.
- Nets initially derive from exact case-sensitive named pad memberships. Schematic net identity, net classes, holes, circular/custom pads, paste/courtyard editing and reusable library definitions follow in later tickets.

## Verification

Exercise save/load byte stability and version protection, invalid native geometry/IDs, atomic command failures, identity preservation, stale guards, deletion/undo/replay, exact transformed rectangle corners, and desktop/MCP parity. Run all Linux repository gates.
