# Explicit assembly placement calibration

Ticket: T-0100 · Story: S-074 · Status: proposed for independent review

## Public sources and scope

Reviewed 2026-10-11, using public documentation only:

- [JLCPCB placement file guidance](https://jlcpcb.com/help/article/pick-place-file-for-pcb-assembly)
  describes reference, centroid coordinates in millimetres, side and positive
  counterclockwise rotation. This establishes useful placement data, without
  establishing a universal package zero or complete bottom-side convention.
- [JLCPCB assembly FAQ, questions 2–3](https://jlcpcb.com/help/article/pcb-assembly-faqs-part-2)
  explains that design and assembly zero orientations can differ, and that tape
  orientation may depend on the exact component. A package name is insufficient
  evidence for automatic calibration.
- [PCBWay assembly requirements](https://www.pcbway.com/assembly-file-requirements.html)
  requests centroid data for surface-mounted parts and distinguishes surface
  mount, through-hole and hybrid assembly in its BOM requirements.
- [KiCad library convention F6.2](https://klc.kicad.org/footprint/f6/f6.2/)
  recommends a body-centre anchor but documents exceptions. An existing component
  origin cannot be assumed to be its assembly centroid.
- [RFC 4180 section 2](https://www.rfc-editor.org/rfc/rfc4180#section-2)
  describes CSV quoting and record conventions; use the existing shared writer.

Implement independently from this specification. No proprietary implementation,
library, package asset or manufacturer database is used. The new export is a
generic, explicitly calibrated placement table, without manufacturer acceptance,
reel-orientation verification, polarity checking or machine compatibility claims.

## Optional persistent metadata

Extend existing `AssemblyMetadata` with two optional fields, omitted when absent:

```text
mounting: surface_mount | through_hole | hybrid
placement: {
  centroid_local: { x: integer nanometres, y: integer nanometres },
  zero_axis_mdeg: integer millidegrees
}
```

`mounting` and `placement` may be stored independently. All fields inside a
present placement object are required. Unknown fields or mounting values, floats,
partial objects and invalid scalar ranges are rejected. There is no implicit
mounting value, centroid, angle, body-centre calculation or package-zero lookup.
The explicit centroid may lie outside the footprint body.

Centroid local X/Y each lie within ±10,000 mm, inclusively. The zero-axis angle
lies within 0..359,999 millidegrees. Use integer model values exclusively. The
desktop converts finite decimal millimetres/degrees at the input boundary; it
rounds to the nearest nanometre/millidegree and rejects values outside the stated
ranges, including a degree value rounding to 360,000 millidegrees.

Keep the existing model-wide limits of 100,000 metadata-bearing components and
2 MiB aggregate sourcing text. The new fields add no strings. Metadata-free boards
retain their existing geometry limits. Borrowed validation precedes mutation,
snapshot cloning, native serialization and log serialization. Default metadata
normalization considers the new fields: populated metadata with mounting or
placement must not be discarded as empty.

The existing guarded `SetPartAssembly` command sets or clears the complete
metadata value; query results expose the optional fields. Undo, redo, grouped
replay, native save/open and ordinary part transformations preserve them.
Desktop updates always include the complete originally inspected Part and the
full original guard. Rejected or stale changes leave the session unchanged.

## Coordinate and rotation contract

The model uses global X right and Y down. The local centroid uses the same
unmirrored footprint frame as explicit pads. Transform a local centroid by
mirroring local X for a bottom component, applying the existing component
counterclockwise rotation in the Y-down frame, then translating by the component
origin. Emit a common top-view coordinate system for both sides:

```text
CSV X = transformed model X
CSV Y = -transformed model Y
```

The output origin is the model global origin `(0,0)`. There is no board-bounds
offset and no additional bottom-side coordinate reflection. Output coordinates
must fit ±99,999.999999 mm. Quarter-turn transforms use checked integer
arithmetic. Other angles use the existing floating-point I/O transform, check
finite/range values before conversion, and round to the nearest nanometre (half
away from zero). The stored model remains integer-based.

`zero_axis_mdeg` describes an explicitly chosen assembly orientation marker in
the **unmirrored local frame**: 0 points along +X; 90,000 points along -Y. This is
a local marker direction, not an angle to subtract from the current component
rotation and not an inferred manufacturer zero. Its final global top-view
direction is emitted as positive counterclockwise rotation in [0,360) degrees:

```text
top:    (part_rotation_mdeg + zero_axis_mdeg) modulo 360,000
bottom: (part_rotation_mdeg + 180,000 - zero_axis_mdeg) modulo 360,000
```

The bottom formula follows reflection of the local marker vector; it is an
explicit Viamind generic convention. For a bottom component with part rotation
zero and marker zero, the output rotation is 180 degrees. Vendor documentation
above does not establish this formula as a vendor convention.

Independent asymmetric fixture: origin `(10,20)` mm, part rotation 90 degrees,
local centroid `(2,1)` mm and local marker 37.125 degrees produce top
`(11,-18)` mm / `127.125` degrees and bottom `(11,-22)` mm / `232.875` degrees.
Also verify a non-cardinal rotation with independent vector arithmetic and a
one-nanometre coordinate tolerance, plus overflow/range rejection.

## Generic calibrated placement CSV

Add shared assembly kind `assembly_placement`, labelled “Calibrated placement”.
It is available through the existing CLI, typed MCP export request and desktop
CSV catalogue. Each included component produces one row, sorted by designator
then stable component UUID. Header fields in order:

```text
Designator
Component ID
Value
Footprint
Mounting
X (mm, global)
Y (mm, up)
Rotation (deg CCW, top view)
Side
```

These are fields, not a literal unescaped header line. Use existing RFC-style
escaping, UTF-8 and record endings. Emit coordinates with six fractional decimal
places, angle with three, explicit enum mounting tokens and `top` / `bottom`.

Include every component explicitly marked populated or with absent assembly
metadata (the existing default population behavior). Every included component
must have both explicit mounting and placement calibration; missing data rejects
the complete export before publication. Skip unpopulated components before
calibration/reference requirements. Surface mount, through-hole and hybrid rows
are all included and distinguished by the mounting field. Empty/all-unpopulated
boards produce the header only. Result component count is the included subset.

Use the existing populated-template designator syntax and case-insensitive
collision checks. Do not require MPN, manufacturer, supplier or SKU for this
generic table. Existing BOM, component-origin placement, sourcing BOM and both
vendor BOM templates retain their exact bytes and population conventions.
Existing generic and checked saved output-job profiles remain unchanged and
reject the new kind rather than silently expanding their fixed release contents.

## Bounded capture and publication

Keep the existing 100,000 source-component, 4,096-byte per-header-field, 2 MiB
aggregate header text and 32 MiB CSV limits. Charge output before each append.
Reject invalid included calibration and reference data through borrowed
preflight before allocating keys, references or copied records.

Capture only needed headers, transform, population, mounting and calibration for
the new kind. Unused sourcing strings, pads, tracks, schematic data and other
unrelated content must not be copied or validated by this export. Old sourcing
and vendor BOM captures copy only their existing sourcing/population fields;
geometry captures continue to exclude all assembly metadata. Kind-aware capture
must prevent producing a calibrated table from a snapshot missing calibration.
Audit every metadata-copy path and explicit AssemblyMetadata constructor.

Retain full-cursor validation before admission, the shared two-slot permit held
by the actual worker, and the existing retained single desktop worker. Rendering,
staging and fsync occur outside the engine mutex. NEW-file default, explicit
overwrite, captured/current native-path protection and save/open lifecycle
serialization remain shared with existing CSV exports. Errors and cancellation
must not publish partial files or release admission while work continues.

## Compatibility and desktop

Optional fields serialize after the old fields and are absent for old projects,
preserving old canonical bytes. Native and event-log minimum version is 9 when
either new field appears, composed with every other document feature by maximum.
Sourcing-only projects remain minimum 7; named-net policy introduced by T97 is
minimum 8. Event-log feature checks cover base, before, after and intermediate
states, including transactions clearing the last calibration. Reject unsupported
downgrades before writing bytes. Tests use the current `LOG_VERSION` for source
headers while retaining explicit historical target versions.

Extend existing component properties with explicit mounting selection, optional
calibration enablement, local X/Y mm and zero-axis degrees. Display the local
marker/top-view convention and an honest generic-export limitation. Preserve
unset values until enabled; applying unrelated sourcing edits must preserve
existing calibration. Clear metadata clears all fields explicitly. Use bounded
inputs and vertical scrolling so Apply/Clear and export controls remain reachable
at the supported compact viewport. No new background work runs in the frame.

## Required verification

- Model bounds, absent/default/partial/unknown data, mounting-only and
  calibration-only persistence, old canonical bytes and feature-version gating.
- Guarded command/query parity, stale two-client refusal, full undo/redo/replay,
  sequential grouped changes and native/log round trips including last-field
  clearing. Invalid changes are atomic and queries validate before cloning.
- Independent top/bottom asymmetric and arbitrary-angle CSV parsing; transformed
  centroid and direction, Unicode/quotes/newlines, deterministic ordering,
  population, missing calibration, invalid references, collisions, overflow,
  output budget and unused huge/invalid sourcing exclusion.
- Exact old CSV and saved-job bytes unchanged, CLI/shared MCP/new desktop file
  bytes equal, NEW/overwrite/native lifecycle protection retained.
- Actual compact egui input for setting calibration and selecting/exporting the
  new kind, plus readable native framebuffer evidence with complete session
  invariants and explicit provenance. No vendor or cadence assertion.
- Linux fmt, strict all-target workspace Clippy, full workspace tests/build,
  regenerated actual MCP catalogue reference and exact-head CI before merge.

Full manufacturer acceptance, sourcing validation, mounting geometry/pin types,
automatic centroid extraction, polarity inspection, feeder/reel data and new
saved-job assembly profiles remain outside this increment.
