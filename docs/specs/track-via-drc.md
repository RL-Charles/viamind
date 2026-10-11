# Scoped batch DRC for straight tracks and through vias

Clean-room behaviour spec, written before implementation. Public primary sources:
[PCB editor DRC rules](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rules-checking)
describe independent copper clearance, board-edge clearance, hole clearance,
drilled-hole spacing, minimum hole size and annular-width constraints;
[R-tree envelopes](https://docs.rs/rstar/latest/rstar/struct.RTree.html)
describe envelope candidate queries and bulk loading. Geometry formulas below are
derived independently from distances between elementary sets. No existing EDA
implementation is copied.

## Prerequisites and preserved behaviour

Implementation begins only after the straight-track/through-via model and scoped
pad DRC are merged. Extend the same immutable captured-project checker and typed
`run_drc` input used by the desktop; retain source cursor, stable ordering, explicit
coverage, limits, stale-result presentation and no document/history mutation.
Persisted integer Nm remains the source of truth. Revalidate imported/native
geometry and reject unsupported invalid geometry with actionable object IDs.

## Copper sets and layers

- Rectangular SMD pads retain the existing mirrored/rotated rectangle geometry and
  their single top/bottom side.
- A straight track is the closed capsule formed by sweeping a disk of radius
  `width / 2` along its finite centre segment. Its end caps are round; use its
  declared top/bottom layer only.
- A through via is concentric annular copper with outer radius `diameter / 2`
  and inner radius `drill / 2`, present on both top and bottom copper. The open
  drill interior contains no planar copper. Copper wholly within a via drill must
  not be mistaken for overlapping via copper.

Check each object pair once when their copper layers intersect. Named equal nets
are exempt from electrical copper clearance. Different named nets, or either net
unassigned, require clearance; touching/overlap violates even a zero rule. A via
and a bottom pad share copper despite the via also spanning top; opposite-layer
tracks and opposite-side SMD pads do not share copper. Through-via pairs always
share layers. No additional duplicate issue is emitted for a pair's second layer.

## Independently derived distances

Use actual shape distance, not an AABB or a tessellated-circle approximation.
Segment/segment and segment/rectangle distances combined with capsule radii cover
track pairs and pad/track pairs. For a connected copper shape X and annulus at C,
let `m` and `M` be its minimum/maximum radial distances to C. Its distance to the
annulus `[r, R]` is `max(m-R, r-M, 0)`. Rectangle maximum radial distance is at a
corner; capsule maximum is the more distant endpoint plus radius. Minimum radial
distance is point-to-rectangle or point-to-segment minus capsule radius, clamped
to zero. For two annuli with centre distance d, the copper distance is
`max(d-R1-R2, r1-d-R2, r2-d-R1, 0)`. Include concentric and nested-hole cases.

Use bounded ephemeral f64 Nm computation with the existing 0.001 Nm tolerance and
whole-Nm measured values rounded down. Half-Nm radii and exact cardinal transforms
remain representable. Coordinate limits prevent loss of tiny shapes at extreme
legacy origins. A finite/nonnegative rule is converted once to Nm before checking.

## Substrate and physical drill rules

All supported copper must remain inside the polygon substrate and outside its
cutouts, then meet copper-to-edge clearance. Evaluate actual capsule or annular
copper against polygon edges, including round caps, concave notches and enclosed
cutouts. A polygon boundary wholly inside a via drill does not intersect annular
copper. Boundary tangency is allowed only for zero edge clearance. Do not treat a
via centre or its outer disk as a substitute for annular containment.

Add bounded configurable minimum via drill diameter and minimum radial annular
width `(diameter-drill)/2`. Equality meets a minimum. Defaults are examples rather
than manufacturing recommendations. Independently configurable drilled-hole
spacing checks the distance between through-via hole disks, regardless of net;
same-net holes can still be mechanically too close. Foreign copper-to-hole
clearance checks a via drill against other different-net/unassigned copper on a
shared layer; exempt the via's own annulus and same assigned-net copper so a
connected track may terminate at its via. Reject nonfinite/negative/oversized
settings without changing the session. Hole diameters and annular widths retain
their exact half-Nm comparison before measured values are rounded for reporting.

## Results, coverage and bounded work

Generalise references to tagged pad, track and via IDs; pad references retain
part/pad ID, designator and number. Track/via references carry their object ID,
layer coverage and net. Desktop issue selection targets actual persisted IDs
using the shared selection action, while pad selection continues to select parts.
Stable kind/object/boundary ordering must be independent of source insertion order
and spatial index iteration. Measured/required Nm and actionable messages identify
minimum drill, annular, hole-spacing and foreign-hole-copper issues separately.

R-tree broadphase uses outward-rounded copper/hole bounds enlarged by relevant
rules. Count all examined candidates, including exempt net/layer pairs, in the
bounded work budget. Preserve explicit result/work truncation and exclusion detail
limits. Coverage reports pads, tracks, vias and holes actually checked, plus the
precise checks not performed. Remove tracks/vias from exclusions only when their
checks really execute. Legacy preview pads, pours, mask/silkscreen, schematic
connectivity/unrouted checks, unsupported via types and remaining fabrication
constraints remain excluded. No full fabrication approval or automatic online
timing claim is introduced.

## Verification

Fixtures cover capsule end-cap contact and diagonal distance, pad/track true
geometry versus overlapping AABBs, opposite layers, via on both layers, same-net
exempt copper with independent hole-spacing failures, unassigned nets, annuli with
copper inside the drill or nested holes, concentric and offset via pairs, exact
thresholds/odd Nm widths, round-cap board intrusion, annular cutout topology and
minimum drill/annular rules. Native roundtrip and repeated reordered snapshots
must return equal results; invalid settings leave session state intact. HTTP
clients and real egui interactions verify typed rule parity, mixed object IDs,
selection and stale results. Run all four required Linux workspace gates.
