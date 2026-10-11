# Desktop fabrication snapshots

Choose **Board → Export fabrication snapshot…**, enter a **new directory** in an
existing parent directory, and choose mask expansion from 0–10 mm. Default 0 means
mask openings equal pads; through vias are untented. Existing files, directories
and the active native project are preserved. There is no overwrite option.

One retained worker performs the export. Inputs and Export are disabled while it
runs; closing the form retains the job, and reopening it shows the result. Thread
startup failures and worker disconnection produce readable errors. Inputs remain
available for correction and retry. Application exit can interrupt the worker.
The form scrolls vertically, including at the supported 640×400 minimum viewport.

Success shows the destination, seven package files and the complete source stream
UUID/sequence. A later design edit displays a newer-edit notice. The output is the
captured snapshot; exporting changes no design, selection, history or dirty state.

The desktop and MCP share the typed `FabricationExportRequest<Cursor>`, existing
JSON schema, bounded `FabricationSnapshot` and unchanged writer. An optional full
`expected_cursor` is checked before input validation, budgets and admission. A
stale request must be refreshed; no output is published. The process admits at
most two manufacturing snapshots, validates counts/text before cloning board
geometry, and excludes unrelated schematic sheets. Limits remain 200000 board
primitives, 4096 bytes/string, 2 MiB aggregate text and 128 MiB output.

```json
{
  "path": "/existing/parent/new-fabrication",
  "mask_expansion_mm": 0.05,
  "expected_cursor": {"stream_id": "<current UUID>", "sequence": 12}
}
```

`export_fabrication` returns `path`, `manifest` and `exported_cursor`. Headless CLI:

```sh
viamind --export-gerber --open project.viamind.json \
  --output /existing/parent/new-fabrication --mask-expansion-mm 0.05
```

Supported files are two outer Gerber X2 copper layers, two mask-opening layers,
polygon profile with cutouts, through-via XNC drill, and a manifest. Coordinates
share the model origin with X right/Y up, viewed from above; bottom files are not
mirrored. Geometry requires explicit rectangular SMD pads, straight outer tracks,
through vias and an explicit polygon outline. Legacy graphical previews, inner
copper, pours, paste, silkscreen, component holes and fabrication certification
remain outside this output. A package is not manufacturer acceptance.

See the [format specification](specs/fabrication-gerber.md),
[control specification](specs/desktop-fabrication-controls.md) and
[portable Linux/native verification](desktop-fabrication-verification.md).
