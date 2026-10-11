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
show the destination, actual file count and complete stream UUID/sequence cursor. A
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

A default generic version1 release contains:

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
previews, inner copper/pours, paste/silkscreen/component holes, manufacturer presets
and certification remain unsupported. Default versions 1/2 omit sourcing identifiers;
version 3 adds explicit populated sourcing as described below. Optional populated
calibration and checked validation are described below; views and ZIP are not included.

Job saves require a NEW file; releases require a NEW directory, both in an
existing parent. Existing files/directories/symlinks and active native paths are
preserved. Files and staging directories are synced before atomic Linux
no-replace publication. Failures publish no partial release and ordinary staging
is cleaned. A process interruption can leave a `.viamind-output-job-*` staging
directory. Atomic visibility does not promise durability through power loss.

See [Linux and native verification](saved-output-jobs-verification.md) for recorded artifacts, limitations and reproduction.

## Optional calibrated placement (version2)

The desktop **Include calibrated placement companion (version 2)** checkbox is off
by default. Selecting it and saving to a NEW job file writes this explicit opt-in:

```json
{
  "format": "viamind-output-job",
  "version": 2,
  "name": "Explicit calibrated release",
  "profile": "generic_outer_2layer",
  "mask_expansion_mm": 0.0,
  "include_calibrated_placement": true
}
```

Version1 forbids this field; version2 requires true. False, null, unsupported fields
and missing opt-in reject. Clearing the checkbox emits the unchanged version1
configuration. Loading restores the saved choice; Run always reads the saved file.
The same opt-in works with [checked jobs](checked-output-jobs.md) and their required
numeric checks. Generic releases contain11 files by default or12 with the companion;
checked releases contain12 by default or13 with it.

The additional fixed `assembly/calibrated-placement.csv` has exact standalone
[calibrated placement](calibrated-assembly-placement.md) bytes. Every populated
component requires explicitly entered mounting, local centroid and generic zero-axis
marker. DNP components are excluded; all explicit mounting technologies are included.
Incomplete calibration or invalid/colliding included references refuses the whole
release. Empty population produces a header-only companion. No vendor/package zero,
polarity, reel orientation or centroid is inferred.

Existing BOM and origin placement continue to inventory **all components** with their
original bytes. They are not a matched populated assembly set. The optional companion
uses the separately declared global X-right/Y-up, CCW common top-view marker convention,
including the explicit bottom reflection. It is not a manufacturer submission guarantee.
The version2 release manifest records its fixed path, included component count and
coordinate convention and explains the population distinction. The fabrication manifest
and supported fabrication bytes remain unchanged.

A single retained manufacturing permit and one board/rules capture supply all files
and checked DRC. Borrowed geometry/text/policy and calibrated reference/scalar validation
precede copying. Only fixed population/mounting/calibration values are retained; unused
sourcing strings and schematic sheets are excluded. Existing32MiB CSV/192MiB release
bounds and atomic NEW root apply to the entire composition; no partial success exists.

## Matched populated generic BOM (version 3)

Select both **Include calibrated placement companion (version 2)** and
**Include matched populated BOM (version 3)**, then save to a NEW job file. The
matched option requires calibration; clearing calibration also clears the local
matched choice. Load restores both saved options. The explicit descriptor is:

```json
{
  "format": "viamind-output-job",
  "version": 3,
  "name": "Matched populated release",
  "profile": "generic_outer_2layer",
  "mask_expansion_mm": 0.0,
  "include_calibrated_placement": true,
  "include_populated_bom": true
}
```

Version 3 requires both flags present and true. Versions 1/2 forbid
`include_populated_bom`; false, null, missing/duplicate fields and unsupported
combinations reject. Both existing profiles support this opt-in, including all
required numeric checks for the checked profile. Generic releases contain 13 files;
checked releases contain 14.

The additional `assembly/populated-bom.csv` contains exactly the component-ID set
in `assembly/calibrated-placement.csv`. Every populated component requires explicit
mounting and calibration. All mounting types are included; no manufacturer-specific
SMT filter is applied. DNP components are excluded from both companions. Older generic
BOM/origin-placement inventories still include all components with unchanged bytes.

The BOM has one component per row, quantity 1, in ascending stable UUID order:

```csv
Designator,Quantity,Value,Footprint,Mounting,Manufacturer,MPN,Supplier,Supplier SKU,Component ID
```

Values and optional identifiers are exact user-entered fields with standard CSV
escaping, UTF-8 and CRLF rows. Missing identifiers remain blank; no fallback or
procurement lookup occurs. Placement keeps its existing reference ordering and
calibrated top-view convention. Shared membership does not imply shared row order.
Empty population produces headers only in both companions.

The version 3 manifest adds `populated_assembly` with both fixed paths, included count
and `component_ids_sha256`. The digest streams ASCII
`viamind-populated-components-v1`, a zero byte, then each included UUID's 16 network-order
bytes in ascending UUID order. It identifies the component set, not CSV content or
manufacturer acceptance. Existing manifest file entries still record byte lengths
and the complete source cursor. Versions 1/2 omit this summary and preserve their bytes.

Borrowed included-only sourcing validation precedes calibration-reference allocation
and the single geometry capture. Each present identifier is 1–512 UTF-8 bytes without
controls or edge whitespace. Included sourcing has an additional 2 MiB aggregate limit;
DNP sourcing is excluded. Version 3 retains only needed included sourcing plus fixed
population/mounting/calibration values, under the same single manufacturing permit.
The existing CSV/release limits, same-snapshot checked DRC and atomic NEW root apply.
No centroid, polarity, stock, vendor zero or submission acceptance is inferred.

See [production/native verification](matched-populated-output-job-verification.md)
for independent CSV/digest parsing, prior version 1/2 byte comparisons and reproducible
shared CLI/MCP/desktop proof.

## Inspect an existing release

Use **File → Inspect saved release…**, MCP `inspect_output_release` with
`{"path":"/absolute/existing/release"}`, or the display-free CLI:

```sh
viamind --inspect-output-release /absolute/existing/release
```

This read-only Linux inspector accepts bounded fixed-layout releases from versions
1–3 and reports historical project/cursor facts independently of the current board.
It checks strict settings/manifest consistency, regular-file lengths and observed
path stability. When present, it validates calibrated rows; version 3 additionally
compares populated BOM membership/shared fields and the component-set digest.
Startup read-only MCP admits this observation. The desktop reads in a retained
background worker with disabled pending controls and scrollable results.

It does not authenticate file content, recompute geometry/DRC or establish manufacturer
acceptance. Same-length changes to unchecked files can pass. Unlisted entries are not
inspected, independent reads are not an atomic cross-file snapshot, and files can change
after return. Per-file inspector caps (32 MiB per CSV, 64 KiB settings/manifests,
16 MiB validation) can refuse otherwise valid larger exports. See the
[bounded contract](specs/saved-release-inspection.md) and
[production/native verification](saved-release-inspection-verification.md).
