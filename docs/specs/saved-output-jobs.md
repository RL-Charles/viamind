# Saved manufacturing output jobs

Independent behavior for T-0084, specified before code. This is a Viamind JSON
format; it does not read proprietary output-job or other products' jobset files.

## Public sources

- [Public output-job documentation](https://www.altium.com/documentation/altium-designer/preparing-for-manufacture/output-jobs): saved reusable output settings, grouped outputs and folder containers. Used for behavior only; no code/assets/file-format compatibility.
- [KiCad9 CLI jobsets](https://docs.kicad.org/9.0/en/cli/cli.html#_jobset_commands): running a saved jobset against a project from the command line and reporting failures.
- [RFC8259](https://www.rfc-editor.org/rfc/rfc8259): UTF-8 JSON objects/strings/numbers; implementations may impose input limits.
- [Rust Read::take](https://doc.rust-lang.org/std/io/trait.Read.html#method.take): bound the bytes read before parsing.
- [rustix RenameFlags::NOREPLACE](https://docs.rs/rustix/latest/rustix/fs/struct.RenameFlags.html#associatedconstant.NOREPLACE): publish a new directory without replacing an existing destination.
- [Rust thread Builder::spawn](https://doc.rust-lang.org/std/thread/struct.Builder.html#method.spawn): recoverable named worker creation.
- Existing [generic assembly](assembly-csv.md) and [outer Gerber/XNC](fabrication-gerber.md) specs define unchanged file contents and exclusions.

## Detached configuration

A reusable job is a separate UTF-8 JSON file, independent of native project state:

```json
{
  "format": "viamind-output-job",
  "version": 1,
  "name": "Manufacturing release",
  "profile": "generic_outer_2layer",
  "mask_expansion_mm": 0.0
}
```

Require the declared format/version/profile and a nonblank name of at most256
UTF-8 bytes, with no control characters. Mask expansion defaults to0 and must be
finite from0–10mm. Reject unknown/duplicate fields, wrong types, unsupported
versions/profiles, malformed/deep JSON and files over64KiB. Read at most64KiB+1
before parsing; require a regular non-symlink job file and avoid blocking on FIFO
or device input. Canonical saves use the typed field order and a trailing newline.
No project paths, output paths, arbitrary filenames, commands, variables, scripts,
remote destinations or extension hooks are interpreted from the configuration.

Save only to an explicit NEW file in an existing parent, with atomic no-clobber
publication and native-project protection. Read/save configuration changes no
project/history/selection/dirty state. A job can be reused with another native
project. Destination and optional expected source cursor belong to the run request,
not the saved job. The descriptor copied into a release is the exact validated
canonical configuration used for that run, even if its source file later changes.

## Coherent bounded run

Load/validate the bounded job outside the engine lock. Under one lock, check the
optional full stream UUID/sequence cursor, component/header and board/text budgets,
then capture one board-only `FabricationSnapshot`, actual cursor and native path.
At most100000 components and the existing200000 fabrication primitive/4096-byte
string/2MiB aggregate text limits apply before cloning. Unrelated schematic sheets
are never cloned or validated. One shared manufacturing permit is retained through
all rendering/staging/publication; no second snapshot or permit is taken to render
BOM/placement from the same captured board. Admission remains nonblocking with
at most two manufacturing snapshots per process. Cancellation retains admission
until an already running blocking worker finishes.

Run the existing generic BOM and placement renderers and existing supported
fabrication renderer against that same snapshot. Preserve every accepted output
byte, CSV escaping/grouping/origin conventions, Gerber transforms and XNC tools.
Generated release contents have fixed paths:

- `assembly/bom.csv` and `assembly/placement.csv`
- `fabrication/` containing the existing seven files, including its unchanged manifest
- `job.json`, the canonical validated job descriptor
- `release-manifest.json`, versioned metadata with project identity, full source
  cursor, fixed relative filenames/byte lengths and explicit supported exclusions

The release contains eleven files. No native source document or absolute source
path is copied. Bound the complete release to192MiB, including descriptors and
manifests; retain each existing32MiB CSV and128MiB fabrication limit. Check bounds
before appending/allocating output. No unbounded file list or configurable path
can enter publication.

Create and sync files in a new sibling staging directory, including its bounded
subdirectories, then atomically publish the complete NEW release root with Linux
no-replace rename. Existing files/directories/symlinks and native project paths
reject. Any validation/render/write/collision failure leaves no final release;
staging is cleaned on ordinary failure. Process interruption may leave a clearly
named staging directory. Promise atomic visibility, not power-loss durability.
No overwrite option or partial success exists for a release.

## Access and desktop

CLI `--run-output-job JOB --open PROJECT --output NEW_RELEASE` performs the same
headless operation. MCP exposes bounded load/save and `run_output_job` with the
same typed requests, optional full cursor, returned release manifest/path/cursor
and limits. These are catalogued export actions, not design mutations.

A desktop saved-output-job form provides bounded name/mask/job/release fields,
explicit NEW configuration save, load and run. Run reads the saved descriptor;
unsaved form changes are identified clearly. One retained fallible named worker
handles I/O off the UI thread; pending controls disable, hidden polling retains
completion, and errors preserve inputs. A scrollable640×400 form reports complete
source cursor, destination/file count and a newer-edit notice. Preserve all other
export menus, synchronization/ERC/cross-probe and routing controls.

## Verification and limits

Round-trip strict configuration, oversize/special/deep/unknown/duplicate inputs;
actual compact desktop save/load/run, library/CLI/live-MCP byte parity and all
fixed paths; one coherent snapshot despite concurrent edits; stale/busy/count/text/
output bounds; admission retained on cancellation; native/collision/raced creation
and failed staging with no partial release; full project/session/status preserved.
Run Linux fmt, strict workspace all-target clippy, workspace tests/build and fresh
CI. Retain exact production CLI/native HTTP artifact provenance, readable Cua
capture and portable reproduction sources; no native raw input claim without a
supported route.

Only current generic assembly and supported outer Gerber/mask/profile/through-via
outputs are bundled. Legacy previews, inner copper/pours, paste/silkscreen/component
holes, sourcing/population/centroid metadata, vendor presets/certification, views,
ZIP, validation gate automation and arbitrary output pipelines remain open. The
story's full release scope remains limited by the existing exporters.
