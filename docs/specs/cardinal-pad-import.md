# Exact cardinal rectangular pad orientation import

T-0094, S-079 with S-083 fidelity cross-reference. Initial base
`cc1813f2c734eb808d702002da4bcc2c93bf6bf3`. This specification precedes
implementation and receives independent root review before parser changes.

## Public sources and restricted interpretation

The public [common S-expression format](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/)
defines containing-object coordinates in millimetres, the optional positional
angle in degrees, and rectangular pad size X/Y. The
[standalone footprint format](https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/)
and [board format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)
use those shared footprint/pad definitions. Only these public descriptions and
original authored fixtures inform this independent increment. No external EDA
implementation, library, fixture or artwork is read or copied.

Extend the two existing bounded parsers from zero-angle pads to exact cardinal
rectangular SMD pads. The containing footprint must still be front F.Cu with
zero rotation. A standalone footprint's optional root placement must remain
zero/origin. No bottom/reflection, combined containing/pad orientation or angle
reference inference is introduced. Rectangles at opposite cardinal directions
have equal geometric sets, so this subset does not require choosing a rotation
handedness absent from the public format description.

## Exact input and normalization

Each pad requires its existing exact `(at X Y [ANGLE])` with two or three scalar
arguments and no children. Absent ANGLE means zero. ANGLE is optional-minus
ASCII decimal: one or more integer digits, optionally followed by a decimal
point and one or more digits. Every fractional digit must be zero. Extra zero
places are accepted exactly within existing string bounds. No plus prefix,
whitespace, exponent, NaN, infinity, trailing point or unsupported punctuation.
Checked integer parsing precedes normalization. Reject values outside inclusive
**-360 through 360 degrees**, including otherwise equivalent turns. Reject all
values that are not exact multiples of 90. No rounding or epsilon comparisons.

Normalize the bounded signed integer with Euclidean modulo 360. Supported
canonical values are 0, 90, 180 and 270 degrees:

| Canonical pad angle | Native center | Native width | Native height |
| --- | --- | --- | --- |
| 0 or 180 | exact source X/Y | exact source size X | exact source size Y |
| 90 or 270 | exact source X/Y | exact source size Y | exact source size X |

The native pad model stores this equivalent axis-aligned rectangle without an
angle field. All source dimensions and local coordinates remain integer Nm;
no floating-point coordinate, corner or trigonometric conversion occurs.
Swapping odd-Nm dimensions preserves exact geometry even when rectangle corners
lie at half-Nm offsets. Containing footprint origin, side, rotation and native
application placement rules remain subject to their existing checks.

Original pad shape, size, coordinate and layer validation remains authoritative:
positive bounded dimensions, exact decimal Nm, unique literal pad numbers,
rectangular smd only, front copper with optional front mask/paste. Invalid angles
block the whole PCB Apply or detached conversion, never a partial pad list.
Unknown electrical semantics and unsupported containing transforms still block.

## Identity, connectivity and fidelity

PCB imports retain existing strict source part/pad IDs and exact net-table
assignments. Detached conversion still omits source identities and meaningful
source nets block; subsequent native application remains separate and undoable,
retaining existing IDs/nets by exact pad number. This increment creates no new
project model, ID, net, sourcing or stable schematic link policy.

Every-list reports remain deterministic and exhaustive. A supported pad's `at`
item explicitly reports the bounded source numeric angle, canonical angle,
unchanged center and whether width/height are swapped. Its size item describes
exact native dimension normalization when needed. Source angle syntax is not
retained as a native field; the disposition is Imported for its exactly
represented geometric meaning, with normalization explicitly disclosed.
Reports must not suggest that dimensions were rounded or unsupported rotations
were approximated. Rejection reasons identify the angle and supported bound
without copying an arbitrarily long token into diagnostics.

All previous omissions remain visible and full_fidelity remains false. This
increment proves only supported rectangular copper geometry. It does not claim
mask/paste equivalence, pin-one orientation markings, assembly rotation fidelity,
universal library/project compatibility or the parent corpus/DRC criterion.

## Bounds and shared lifecycle

Existing source/path/token/string/depth/list/report/pad/physical-object limits
apply before allocations. Angle parsing adds only bounded linear token work,
checked integer accumulation and constant-time normalization. No per-angle
cache, recursive geometry or new work budget is necessary. Error messages and
normalization reasons use bounded parsed values, not unbounded source lexemes.

Both existing shared engine paths expose this supported subset through the
same preview/apply and preview/new-file commands and matching desktop forms.
Exact source SHA, full project/cursor freshness, clean native PCB replacement,
current native-path aliases, nonblocking same-descriptor regular-file reads,
new-file-only atomic publication and dirty-session/draft preservation remain
unchanged. Source files are never written. Desktop and MCP capability text
must distinguish the containing footprint's zero angle from cardinal pad angles.
No new action without corresponding agent parity is introduced.

## Independent verification before submission

- Original asymmetric signed odd-Nm rectangles cover absent angle and every
  canonical quarter turn, negative equivalents, signed zero and both bounds.
  Independently expect native center/width/height and compare exact doubled-Nm
  corner sets rather than mirroring the parser's swap implementation.
- Both public parsers reject noncardinal/fractional, out-of-bound, malformed,
  exponent/nonfinite/overflow and excessive tokens without partial geometry.
  Retain containing-footprint nonzero/back-side refusal and all previous
  unsupported-semantic, identity, net and work-limit regressions.
- Deterministic exhaustive reports explicitly disclose source/canonical angle
  and dimension normalization, including an asymmetric 90-degree rectangle.
- Shared engine and actual MCP integration verify preserved PCB IDs/nets,
  detached omission, new-file exact round-trip, unchanged dirty session/drafts,
  source-change refusal and separately undoable number-preserving reuse.
- Actual desktop input uses a cardinal original source through existing forms;
  supported preview and output show normalized odd-Nm dimensions. Retain
  readable native report/action/completion evidence and actual production MCP
  checks with exact binary/source/fixture provenance (reserved port7920).
- All Linux formatting, strict workspace all-target Clippy, workspace tests
  and build gates pass under the shared whole-sequence mutex. Fresh combined
  CI and independent review precede parent merge. Do not self-merge or mark done.
