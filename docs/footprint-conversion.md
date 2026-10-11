# Convert standalone footprint files

Choose **File → Convert footprint S-expression…**. Enter one `.kicad_mod`
source and an explicit **new native footprint file** path. Choose **Preview
footprint conversion**, inspect counts and expand the per-list fidelity report.
Choose **Create new native footprint file** only when the supported subset is
accepted. Existing files, source aliases and the active native project cannot
be overwritten. Changed source bytes require another preview.

This creates a detached `viamind-footprint` version 1 library. It preserves the
current project, dirty work, history, selection and editing drafts. It does not
apply geometry to selected parts or make the source an active native file.
The completed report describes the source snapshot used for that conversion.

To reuse the library, open a component's explicit pad editor and choose its
existing native footprint import action. This is a separate guarded, undoable
project edit. Matching pad numbers retain native pad identities and nets; newly
introduced numbers receive fresh IDs and no net. Component placement and side
remain unchanged. Converted source UUIDs are report provenance only.

## Supported subset and omissions

One explicit front `F.Cu` footprint with 1–1024 uniquely numbered `smd rect`
pads, zero pad angle and exact local decimal millimetres. Local coordinates
and rectangle sizes become integer nanometres without rounding. Tested date
headers are `20221018`, `20240108` and `20241229`; this is not universal version
7–9 library compatibility.

Optional UUIDs must be valid, nonnil and unique. Native output has no source
UUIDs, nets, placement, mask/paste memberships, properties, graphics or model
references. Known noncopper art and metadata are explicitly omitted. External
models are never loaded. An absent net or `(net 0 "")` is detached; meaningful
net assignments block conversion. Rotated/back/drilled/other-shape pads,
copper art, board linkage, electrical overrides and unknown semantic objects
also block the entire conversion. No partial library is published.

`full_fidelity` is always false. The report covers every parsed source list
with original token, byte offset, path, optional source UUID and a reason for
importing, omitting or blocking it. This is not a project/library corpus import,
source graphic reproduction or fabrication equivalence claim.

## Bounds and agent parity

The source must be a regular UTF-8 file ≤1 MiB. Parser limits include depth 32,
100000 tokens, 20000 lists/report items, 4096-byte decoded tokens and at most
1024 pads. Paths are ≤4096 encoded bytes; native output is ≤1 MiB. Malformed,
resource-excessive and observed changing sources reject without publication.

Agents use the same `preview_footprint_conversion` and `convert_footprint_file`
methods. Conversion takes `path`, the exact lowercase `expected_source_sha256`
returned by Preview, and `destination`. It rereads and reparses source bytes
before atomic new-file publication. Existing `import_footprint` is the separate
explicit application command.
