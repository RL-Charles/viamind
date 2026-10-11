# Saved manufacturing output jobs

Save reusable manufacturing settings outside the native project. A job bundles the
existing generic BOM/placement and supported outer Gerber X2/XNC outputs from one
board snapshot. These exports do not certify a board for fabrication or assembly.

## Desktop

Choose **File → Saved output jobs…**. Enter a name, mask expansion and a job file
path in an existing parent directory, then **Save NEW job file**. Use **Load job**
to inspect an existing descriptor. Enter a new release directory and choose
**Run saved job to NEW directory**. Run reads the saved file; unsaved form edits
are identified and are not used by Run. To change saved settings, save a new file.

One background worker handles load, save or export. Its bounded controls disable
while pending; closing the window retains the operation and completion. Results
show the destination, eleven files and complete stream UUID/sequence cursor. A
later design edit displays a newer-edit notice. The window scrolls at 640×400.

## CLI and MCP

```sh
viamind --run-output-job release-job.json --open board.viamind.json --output new-release
```

This is a headless invocation. It cannot combine with other export modes,
`--overwrite`, `--headless` or desktop options. MCP exposes `load_output_job`,
`save_output_job` and `run_output_job` with the same typed settings/writer:

```json
{"job_path":"release-job.json","path":"new-release","expected_cursor":{"stream_id":"UUID from get_session_state","sequence":0}}
```

The optional full cursor guards capture. Read/save/run preserve native project,
selection, history and dirty state. A run captures only the bounded board under
one engine lock; unrelated schematic sheets are excluded. One manufacturing
admission slot remains held through rendering and publication, including after
cancellation of an already running blocking worker. At most two manufacturing
snapshots per process are admitted; busy requests reject without waiting.

## Configuration and output

```json
{
  "format": "viamind-output-job",
  "version": 1,
  "name": "Manufacturing release",
  "profile": "generic_outer_2layer",
  "mask_expansion_mm": 0.0
}
```

Mask expansion is finite 0–10 mm and defaults to 0. The nonblank name is at most 256
UTF-8 bytes without control characters. Configuration files must be regular,
non-symlink UTF-8 JSON, at most 64 KiB. Unknown/duplicate fields and unsupported
formats, versions and profiles reject. Linux opens are nonblocking and check the
opened file type. No commands, variables, arbitrary filenames or project/output
paths are interpreted from configuration. Runtime paths are bounded to 4096 bytes.
Canonical saves have fixed field order and a trailing newline.

A release contains:

- `assembly/bom.csv`, `assembly/placement.csv`
- `fabrication/top-copper.gbr`, `bottom-copper.gbr`, `top-mask.gbr`,
  `bottom-mask.gbr`, `profile.gbr`, `plated-through.xnc`, `manifest.json`
- `job.json`, the canonical descriptor actually used
- `release-manifest.json`, project identity, full cursor, relative file lengths
  and exclusions (its file map lists the other ten files)

Existing assembly/fabrication bytes and their coordinate conventions are retained.
Generic placement uses component origins in model X-right/Y-down coordinates;
Gerber/XNC uses the existing common X-right/Y-up top view, with bottom files not
mirrored. See [assembly conventions](assembly-csv.md) and
[fabrication conventions](fabrication-gerber.md).

Limits are 100000 components, 200000 fabrication primitives, 4096 bytes per board string,
2 MiB aggregate board text,32 MiB per CSV,128 MiB fabrication and192 MiB complete
release. Explicit rectangular pads and polygon outline are required. Legacy
previews, inner copper/pours, paste/silkscreen/component holes, sourcing/population
metadata, manufacturer centroids/presets and certification remain unsupported.
Views, ZIP and automatic validation gates are outside this increment.

Job saves require a NEW file; releases require a NEW directory, both in an
existing parent. Existing files/directories/symlinks and active native paths are
preserved. Files and staging directories are synced before atomic Linux
no-replace publication. Failures publish no partial release and ordinary staging
is cleaned. A process interruption can leave a `.viamind-output-job-*` staging
directory. Atomic visibility does not promise durability through power loss.
