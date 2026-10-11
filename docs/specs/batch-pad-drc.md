# Scoped batch design rule checks for explicit SMD pads

Clean-room behavioural spec, before implementation. Public primary sources:
[KiCad PCB editor rules](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html)
for copper/board-edge clearance semantics and cutouts;
[rstar spatial envelopes](https://docs.rs/rstar/latest/rstar/struct.RTree.html)
for spatial broadphase behaviour. No other EDA implementation is copied.

## Scope and input

Run read-only on one captured project snapshot, with source event cursor returned
at the UI/MCP boundary. Check only persisted rectangular SMD pads and validated
polygon outer boundary/cutouts. A part's bottom-side local X is mirrored, then
counter-clockwise rotation and translation use the same +Y-down world convention
as the desktop. Actual oriented rectangle corners are used, including half-Nm
edges for odd pad dimensions. Broadphase axis-aligned bounds never substitute for
narrowphase distance or topology.

Input rules are nonnegative finite millimetres, at most 100 mm each. Example defaults
are 0.2 mm copper and 0.25 mm board-edge clearance, not a manufacturing guarantee.
Convert rules once to integer Nm. Limit result count to 1..10000 (default 1000),
explicit pad geometry to 100000 pads and examined broadphase candidate pairs (including exempt nets/layers) to
1000000. Exceeding the pad limit errors; exhausting result/work limits sets a
visible incomplete/truncated flag and reason. Never silently claim a complete run.

## Checks

1. Same-layer pad pairs on different named nets, or with either net unassigned:
   oriented rectangle Euclidean copper distance less than required clearance is
   a violation. Copper touching/overlap is a violation even for a zero rule.
   Same assigned net pairs and opposite-side SMD pairs are exempt.
2. A pad with any copper outside the outer polygon or inside a cutout is an
   outside-substrate violation. Detect concave boundary intrusion between pad
   corners and a cutout entirely enclosed by a pad. Tangency to a boundary is
   allowed only when edge clearance is zero.
3. Otherwise, the minimum distance to every relevant outer/cutout boundary must
   meet edge clearance. Report the boundary kind/index and required/measured Nm.

Numerical computation uses ephemeral f64 nanometre coordinates for arbitrary
rotation; document coordinates and rule storage remain integer Nm. Cardinal
rotations use exact sine/cosine values. Bounded transforms and a 0.001 nm numerical
tolerance avoid false flags from trigonometric noise; report measured distance
rounded down to whole Nm. Unsupported extreme legacy world positions/rotations are
excluded with stable part IDs and reasons, rather than evaluated with lost precision.

Pad candidates and boundary edges use the existing R-tree envelope index with
outward-rounded bounds. Sort pad references, candidate IDs and final violations
for deterministic output, independent of R-tree iteration and source pad ordering.
Every issue includes project ID, part/pad IDs, designator/number, side and net;
keep both references for a copper pair. No persistent design objects are created.

## Coverage and presentation

Report explicit pads checked, unassigned pad count, parts with no explicit pad
model, unsupported transforms (total exclusion count and first 1000 actionable
ID details, with an explicit details-truncated flag), whether an explicit outline was checked, candidate
pair count and truncation. Always list excluded checks: tracks/vias, pours, drills,
mask/silkscreen, schematic/net connectivity and manufacturing constraints. Legacy
footprint previews are not copper geometry for DRC. No explicit outline means
substrate checks are excluded, never inferred from the preview rectangle.

`run_drc` is a read-only MCP tool with the same typed rule input and report as the
desktop. Capture under the engine lock, release it before geometry work, and return
source cursor so concurrent changes can be identified. Desktop Board → Design rule
check opens rules/results, lists coverage and actionable issues, and marks an old
report stale after design changes. Selecting issue parts uses the existing shared
selection action. Batch checking does not promise automatic online DRC timing or
full fabrication readiness; those remain follow-up work.
