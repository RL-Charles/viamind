# Fabrication overlay Linux verification

Historical source `b2f76539b007544d9fce362540cf4b2417ecd140` on `0fb27a1`
passed fmt, strict workspace all-target Clippy, **598 workspace tests**
(zero failures, one ignored), workspace build and native inspection build.
Five focused actual-input/unit tests passed separately; they are excluded from
the workspace count. The [gate log](verification/t0090-full-linux-gates.log)
and [master provenance](verification/t0090-linux-native.json) retain exact evidence.

The retained production executable passed **32 stdio checks** and **32 actual
desktop HTTP checks with two independent sessions**. The
[portable verifier](verification/t0090-fabrication-overlay.py) independently
expects six layers, eight profile centerlines, a 30-degree rectangular aperture,
exact drill and track coordinates, source hashes and a same-length nine-Nm
source shift. It checks complete session immutability, atomic malformed/unknown
input refusals, FIFO rejection, peer invalidation and same-project stream reopen.
The [stdio](verification/t0090-stdio.json) and
[HTTP](verification/t0090-http.json) reports retain every passed check.

Run against a retained or freshly built production executable:

```sh
TMPDIR="$HOME/.cache/viamind/tmp" PYTHONDONTWRITEBYTECODE=1 \
  python3 docs/verification/t0090-fabrication-overlay.py \
  --binary /absolute/path/to/viamind \
  --fixture fixtures/fabrication/asymmetric.viamind.json \
  --report /absolute/path/to/new-report.json
```

For actual desktop verification, launch that same executable with the fixture
and `VIAMIND_MCP_PORT`, then add `--port PORT` to the verifier. The verifier
adds a test peer track and saves/reopens its temporary project near completion;
use a dedicated instance. All six exact original source files used by the
readable captures are retained in [t0090-exported](verification/t0090-exported).

## Native rendering and actual input

The [wide native capture](verification/t0090-overlay-wide.png) shows the checked
Show control, per-file visibility, source hashes, rounded copper strokes,
rotated apertures, drill boundaries, outer profile and cutout over the board.
The [compact capture](verification/t0090-overlay-compact.png) shows reachable
Inspect and Show controls at 640×400; the dialog covers most of that small canvas.
Scrolling exposes remaining paths and file controls. These captures display
six files and 26 parsed primitives from the original asymmetric fixture.

The production desktop's compositor allocation was only 16×4, so readable
captures use the committed `fabrication_overlay_inspection` example. It delegates
to the production `ViamindApp`, prepares paths and Show through actual egui input,
asserts control bounds and painted geometry, then transfers UI memory into a
fixed native host. The complete session must remain identical every frame.
The master records both distinct executable hashes and exact process/window IDs.
Both native inspections have MCP disabled. The actual production HTTP tests
use the separate production executable on port 7916.

AT-SPI was unavailable: Cua returned degraded window metadata and native pixels.
Those snapshots prove rendering; actual input tests prove the controls and
stale-peer recovery. No operating-system click or frame-rate claim is made.

## Interpretation limits

The viewer accepts the documented current-exporter subset and rejects unknown
semantics. It displays circle/drill boundary rings and prepared rounded stroke
capsules; curves are approximated for screen drawing. Exact file integer geometry
remains in the shared report. Matching coordinate UUIDs establish identity, not
equivalence with the current board. Drill and absent-identity origins are labeled
unverified. Independent reads do not establish an atomic cross-file snapshot.
This evidence does not certify manufacturing or complete a general CAM viewer.

The binary and native evidence retain the historical source above. Later clean
integration must pass fresh combined CI before merge; it does not change the
provenance of these captured binaries.
