# Native symbol library verification

T-0080 adds detached single-unit rectangular symbol definitions, bounded file
loading/export and exact guarded fresh placement. The
[public-source specification](specs/native-symbol-libraries.md) preceded the
implementation. The [original regulator fixture](../fixtures/symbols/original-regulator.viamind-symbol.json)
contains original project data; no external symbol assets or implementation code
were copied.

## Shared behavior and meaningful regressions

Document tests cover strict envelopes and nested fields, detached instance data,
UTF-8 field/aggregate limits, exact odd-nanometre body/pin transforms, new-file
publication and nonregular files. Linux opens use the existing safe rustix
nonblocking API and inspect the same opened descriptor, so a substituted FIFO
cannot hold the retained file worker waiting for a writer.

Engine tests cover fresh symbol/pin identities, one transaction, replay and scoped
creator undo/redo while preserving a peer's PCB edit. Full project/cursor/sheet
guards reject stale edits and same-project reopens. Bounds precede inherited
global identity validation, including an original 100000-identity project that
cannot accept the proposed additional symbol/pins. Source-only export does not
copy or validate unrelated sheet contents. Existing schematic wire regressions
continue to run against the shared extracted preflight.

Two real HTTP clients exercise shared tool discovery, detached file operations,
placement, stale-session rejection and connection-scoped undo. Actual egui input
tests enter a file path and exact decimal coordinates, select 270° then 90° and
mirror, place and undo/redo the resulting instance. They also exercise invalid
sub-nanometre text, peer edits, reopen invalidation, source export/collision and
scrolling to Export/error in a 640×400 viewport. Worker tests cover hidden jobs,
duplicate suppression, disconnection and an older exported cursor after edits.

The initial actual UI run exposed a real 270° label overflow from `u8 * 90`.
The production label now widens before multiplication, and the actual interaction
regression covers it. A later file review exposed the pre-open metadata/FIFO
race; nonblocking descriptor inspection and the no-writer FIFO regressions cover
the fix. Failed runs are retained separately and are not counted as verification.

## Exact production and native evidence

The [master report](verification/t0080-master.json) identifies the verified source,
full Linux gate log, production and inspection binary hashes, CI and captures.
The exact production executable is copied while the shared build lock remains
held. MCP verification uses that immutable copy, independently of later builds.
The later static-walkaround integration preserves byte-identical symbol-library
implementation files and both sets of engine/actions/UI hooks. Historical artifact
identities remain unchanged; fresh combined CI checks the integrated branch.
All four Linux gates pass at the recorded source: **530 passed, zero failed,
one existing timing test ignored**. The production binary passes **29 stdio checks
and 30 actual desktop HTTP checks**. The full gate log and individual check
reports are retained beside the master report.

The [portable script](verification/t0080-symbol-library.py) runs against the
production `--headless --open` stdio process and the actual production desktop's
HTTP endpoint. Its HTTP mode creates two independent MCP sessions. It compares
complete session/model state, checks fresh identities and exact transforms,
protects native/library files, rejects malformed/oversized/nonregular sources,
checks stale full cursors and reopens, and saves/reloads final native data.
Library edits affect only explicitly placed new instances.

The production window is verified through its actual PID/window and HTTP server.
Hyprland tiles that window very small. A separate fixed 1600×1000 inspection
example uses the production `ViamindApp`, engine and library dialog, with actual
egui input loading the original template before launch. Its startup checks that
the complete shared session remains unchanged. This is explicitly a readability
wrapper; it is not presented as the production desktop binary or as native OS
pointer input. Cua captures verify visible content/window identity. AT-SPI was
unavailable and Cua recovered window metadata through its X11 fallback. Pointer,
text and compact-scroll claims come from actual `egui_kittest` interactions.

## Reproduce on Linux

Run fmt, strict workspace all-target clippy, workspace tests and workspace build.
Use a fresh existing temporary parent for `TMPDIR` and separate report paths:

```sh
python3 docs/verification/t0080-symbol-library.py \
  --binary /path/to/verified/viamind \
  --fixture fixtures/schematic/linked-regulator.viamind.json \
  --report /existing/evidence/stdio.json

VIAMIND_MCP_PORT=7906 /path/to/verified/viamind \
  --open fixtures/schematic/linked-regulator.viamind.json
python3 docs/verification/t0080-symbol-library.py \
  --binary /path/to/verified/viamind \
  --fixture fixtures/schematic/linked-regulator.viamind.json \
  --port 7906 --report /existing/evidence/http.json

cargo build -p viamind-ui-egui --example t0080_inspection
/path/to/t0080_inspection \
  fixtures/schematic/linked-regulator.viamind.json \
  fixtures/symbols/original-regulator.viamind-symbol.json
```

Use a fresh production process for HTTP verification because the script makes
guarded design edits and saves its results to an independent temporary directory.
The fixture itself is never overwritten. Binary artifacts are excluded from git.

The parent story's quad op-amp with four units plus power remains open. This
increment does not add multi-unit packages, arbitrary vector graphics, external
symbol formats or live updates of existing instances.
