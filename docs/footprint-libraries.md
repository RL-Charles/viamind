# Reusing rectangular SMD footprints

Select a component and open **Electrical pads**. Apply its pad edits, then
enter a new file path under **Footprint library** and choose **Export applied
footprint**. Parent directories must already exist; existing files are preserved.
Legacy visual preview pads must be applied as electrical pads before export.

For another component, open its pads editor, enter that library path, and choose
**Replace pads from library**. This replaces both the pad draft and committed
geometry. The component keeps its reference, value, origin, rotation and side.
Pads with matching numbers retain their identities and net names; new numbers
receive fresh identities and start unassigned. Removed pad numbers disappear.
The complete change, including the footprint name, is one undoable edit.

If an agent changes the component while the editor is open, import rejects the
stale snapshot. Choose **Reload pads**, review the current component, and retry.
Export captures committed pads; unapplied text edits are excluded.

## Native file

Files contain integer nanometres in the footprint's local frame. Exporting a
rotated bottom component leaves its local geometry unchanged. No UUIDs, nets,
board side or component placement are stored. For example:

```json
{
  "format": "viamind-footprint",
  "version": 1,
  "footprint": {
    "name": "My_SMD",
    "pads": [
      {
        "number": "1",
        "position": { "x": -800001, "y": 0 },
        "width": 900003,
        "height": 950000
      }
    ]
  }
}
```

Only rectangular SMD pads are supported. Files are limited to 1 MiB and 1024
pads; malformed data, repeated numbers, unknown fields and unsupported versions
reject before modifying the design. This native format does not import KiCad
footprints or claim IPC compliance.

## MCP

`export_footprint` and `import_footprint` accept `path`, `expected_project` and
`expected` (the exact component returned by `query_parts`). Obtain the project
UUID from `get_session_state`. Import uses the calling connection's undo scope;
export changes no design, history or dirty state.

`apply_footprint` accepts the same guards and an inline `footprint` definition
instead of a file path. It uses the same atomic application command as import.
