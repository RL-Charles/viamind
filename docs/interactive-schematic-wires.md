# Draw schematic wires on the canvas

Choose a schematic sheet and finish/cancel any object property draft. Click
**Draw wires on canvas**. Attachment preparation runs in one background worker;
this captures the current sheet and event cursor without changing the design.

- Click the first endpoint, then successive targets. Each click adds the cardinal
  bend needed to reach its target. **Horizontal first** controls the bend posture.
- Pins, existing wire ends, junctions and label anchors within 12 screen pixels
  take precedence over the selected integer grid. Their exact coordinates remain
  intact, including off-grid mirrored/rotated pin endpoints.
- **Enter** over the canvas or **Finish wires** commits only clicked vertices.
  The moving hover line is a preview. The whole path is one transaction; Undo
  removes all its segments and Redo restores their identities.
- **Backspace** over the canvas removes the most recent click and its inserted
  bend. **Escape** or **Cancel wires** discards the local path. Middle drag,
  Space+drag and wheel zoom preserve the path without adding vertices.

An interior crossing does not connect wires. A wire endpoint placed on another
wire does connect, including a bend deliberately placed at an intersection.
Existing junctions connect according to native graph rules. The tool inserts no
junction objects, moves no symbols and never repairs existing connectivity.

Any design edit makes the captured path stale. A stale commit rejects unchanged
and retains the preview for inspection/cancellation; cancel and restart to capture
current endpoints. Changing sheet/editor or replacing the document cancels the
old path. Cross-probe and ERC navigation reject while drawing. Board selection,
existing schematic objects, sheet metadata and copper remain unchanged.

## Agent workflow

`snapshot_schematic_wire_sheet` takes `sheet_id` and returns `expected_project`,
`source_cursor` and the exact `expected` sheet. Pass these unchanged to
`draw_schematic_wire_polyline`, with a `vertices` array of integer nanometre
coordinates. For example, a path can go from `{ "x": 1000001, "y": 2000003 }`
to `{ "x": 5000007, "y": 2000003 }` to `{ "x": 5000007, "y": 8000009 }`.
Every adjacent pair must differ and be horizontal or vertical. The same command
and validation serve desktop and MCP. Snapshotting is read-only.

The command requires the initial full stream/sequence cursor, project UUID and
exact sheet. It rejects stale/reopened documents, invalid geometry, oversized
input and failed batches without partial changes. Created wire UUIDs are nested
inside one modified sheet delta; the command's top-level `created` array does
not list nested wire IDs. Query or snapshot the resulting sheet to inspect them.

## Supported limits

Paths have 2..256 vertices, coordinates within ±10000 mm, and at most 255 new
segments. Sheet capture/result limits are 10000 objects, 10000 pins, 4096 bytes
per string and 2 MiB aggregate text; pointer attachment uses at most 30000 indexed
anchors. The available grids are 0.1, 0.25, 0.5, 1 and 1.27 mm.

Commit also validates project-wide identities and references. Before that work,
it requires at most 100000 combined project/sheet/part/pad/hole/copper/schematic/pin
identities including the new wires, 1000 sheets, and 4 MiB aggregate schematic
text with 4096 bytes per string. An oversized unrelated board or sheet can reject
a commit even when the selected sheet snapshot succeeds. This bounded commit
validation runs synchronously; no latency threshold is claimed.

This tool supports cardinal wire creation. Wire dragging, connection-preserving
symbol movement, implicit junction insertion, buses and hierarchy are separate
work. A successful wire transaction is not an ERC or fabrication certificate.

See the [Linux/native verification](interactive-schematic-wires-verification.md)
for source/binary provenance, actual input tests and retained preview captures.
