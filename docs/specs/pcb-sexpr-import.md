# Bounded standalone PCB S-expression import and fidelity (T83)

## Public sources and scope

Independent implementation from these public format descriptions only:

- [Board file format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/):
  board header, canonical layer table, net ordinals, segments and vias.
- [Common S-expression format](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/):
  millimetres, integer-nanometre precision, containing-object coordinates,
  footprint/pad definitions and identities.

No external application code, libraries, examples, symbols or assets are copied.
Fixtures are original small text boards constructed from the documented grammar.
The product module is a generic PCB S-expression importer; the external format
name is used only to identify interoperability. This is a first S079 increment
and S083 reporting foundation. Whole projects, schematics, corpus netlist/DRC
agreement and full fabrication fidelity remain unproven.

## Accepted geometry

Accept one `kicad_pcb` root and explicitly tested date-version headers
`20221018`, `20240108` and `20241229`. Other versions reject with an actionable
message; this whitelist does not promise every feature of any application release.
Require unique unsigned layer ordinals, unique documented canonical layer names,
exact name/type/optional-display-name scalar arity and documented layer types.
Front copper precedes back copper in stack ordering; no undocumented fixed numeric
ID mapping is inferred. Inner copper or unknown canonical layers block Apply.

- `gr_line` and `gr_rect` on `Edge.Cuts` form exact closed loops. Each endpoint
  has degree two; no snapping or healing. The largest loop is the sole outer
  boundary and all others must be strictly internal disjoint cutouts. Existing
  validated BoardOutline rejects open, branching, duplicate, touching,
  self-intersecting, overlapping or multiple-island geometry. Outline stroke is
  reported as unused presentation, not added to substrate dimensions.
- Straight `segment` objects on F.Cu/B.Cu map to round-ended native tracks.
- Circular through vias must span F.Cu and B.Cu, with positive drill smaller
  than diameter. Blind/micro vias, layer removal and altered annular geometry
  are unsupported. Preserve explicit net assignment.
- Footprints initially support only F.Cu, zero rotation and nonempty explicit
  rectangular `smd` pads on F.Cu (plus optional F.Mask/F.Paste). Pad rotation
  must also be zero. This avoids guessing undocumented stored bottom/relative
  orientation conventions. Keep source footprint origin and pad-local offsets;
  world coordinates are origin + local, with positive Y down. Reject unsupported
  transforms rather than bake, reflect or round them.
- Keep footprint name, reference and value; recognized legacy fp_text reference/
  value and newer property fields must agree if both present. Preserve complete
  unique numbered pad lists. No synthetic schematic symbols or stable links.

Parse decimal millimetres directly into checked integer Nm. Reject exponent,
nonfinite, overflow and nonzero sub-Nm digits. Extra zero decimal places are exact
and may be accepted. Every coordinate/dimension also satisfies native ±10000 mm
and positive-size bounds. No floating-point geometry or approximations.

## Identity and connectivity

Preserve native part, pad, track and via UUIDs from `uuid` or legacy `tstamp`; ambiguous duplicate
fields, nil, missing, malformed and repeated IDs reject. Use a fresh native
project UUID. Outline edge UUIDs are retained only as report source IDs; their identity fields are omitted because native outlines have no per-edge identities. Exact outline geometry is imported. References and exact literal pad numbers must be unique in their
native scope. No reference, nearest-point or net-name inference.

Resolve track/via/pad numeric net references through the declared net table.
Ordinal 0 with empty name maps to native None only. Nonzero names are trimmed,
nonempty, control-free and at most 128 characters; duplicate/conflicting ordinals
or names reject because native net names cannot represent distinct same-name nets.
A pad's optional literal net name must agree with its table ordinal. Preserve case
and Unicode exactly. Unused declared nets are reported as not persisted because
the native model stores names on copper objects, not a standalone net registry.

## Complete bounded fidelity report

Every source list/object receives a deterministic source path, token, optional
source UUID, disposition and reason. Imported fields are covered explicitly;
recognized cosmetic metadata/graphics/3D/mask/paste/rules fields are listed as
omitted. Never resolve external library/model paths. Unknown semantic fields,
unsupported copper/pads/outline/layers, missing outline and invalid native data
block Apply. No silent skip, truncated report or partial replacement.

Report imported counts, omission/blocker counts and `full_fidelity=false` whenever
anything is omitted or unsupported. A successful supported-geometry conversion
is not a claim of equal netlist, constraints, DRC or manufacturing output.
Malformed syntax/resource violations may return an error instead of a report;
include the byte offset/source path and remedy where possible. No approximations
are performed in this increment. Geometry blocked by unsupported fields is not
presented as successfully imported.

## Bounds and file behavior

Before allocating or recursing beyond each bound, reject: source >8 MiB, depth
>32, tokens >100000, list/report objects >20000, individual decoded string >4096
bytes, aggregate decoded string text >2 MiB, or combined physical objects >10000.
Native pad limits (1024/part), outline limits (2048 vertices,64 cutouts) and bounded
coordinate magnitudes also apply before expensive validation. Outline validation
work is bounded by its vertex limit. Report paths use child indices and bounded
depth rather than copying arbitrary names. Reject excess diagnostics, never hide
objects behind a result cap.

Read a regular UTF-8 file only. Cap source paths at 4096 encoded bytes before
absolute-path construction or OS calls. Use safe nonblocking Linux open so a
regular-file→FIFO swap cannot stall a worker; validate the opened descriptor. Check file type/size before opening, then verify
opened metadata and read at most limit+1; FIFO, devices, directories, binary and
invalid UTF-8 reject. Recheck after reading. No source writes, external includes,
commands, models or network fetches. Preview returns SHA256 of the exact bytes.

## Shared lifecycle and desktop

MCP and desktop share `preview_pcb_import` and `import_pcb` engine entry points.
Preview reads/parses outside the design mutex and does not change the current
project, shared selection, history, dirty status, event cursor or active path.
Its report is attached to current project UUID/full cursor and source digest.

Apply requires those exact UUID/cursor/digest values, rereads and reparses the
bounded source, rejects any changed bytes or blocker, and rechecks project/cursor
inside replacement. Apply rejects a source equal to the active native save path by absolute/canonical path, even if external changes made that file parse as PCB data. Path resolution occurs outside the engine lock while the lifecycle save mutex is held. Dirty native work rejects: save first. There is no discard
checkbox in this increment. Parse failures and peer edits leave the complete
current session and active save path unchanged. Success is an explicit new-document
lifecycle replacement: new stream, cleared shared selection/history, imported
nonempty document dirty until native save, and no active source save path.
Existing new/open/save semantics stay unchanged. A subsequent native save/reopen
preserves imported supported object identities and coordinates.

Desktop uses one retained background job for preview or Apply. Editing path,
project replacement, peer changes, closing and reopening must not publish old
preview completion into a new context. Apply is enabled only for a current matching
preview without blockers; full guards are checked again by the shared engine.
The accessible report distinguishes exact imported geometry, omissions and blockers.
The source format is never a native Save destination. Existing unrelated drafts
must not be discarded by opening the import flow.

## Evidence

Original small fixtures exercise each supported header/field form, exact odd-Nm
asymmetric geometry/cutout, assignments, identity retention and readable fidelity.
Tests reject unsupported geometry/attributes, conflicting nets, duplicate fields,
malformed identities, excess precision, deep/wide/large strings, oversized geometry,
nonregular inputs, open outlines and unknown future versions unchanged. Shared
engine and actual two-client HTTP tests cover stale previews/source changes, dirty
native protection, project replacement, clean import and native save/reopen.
Actual desktop input/worker tests verify report/Apply freshness. All four Linux
gates and retained native MCP/rendering evidence precede submission; parent corpus
and universal fidelity acceptance remain open.
