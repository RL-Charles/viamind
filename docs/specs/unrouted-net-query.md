# Bounded whole-board unrouted-net observation

Ticket: T-0123, parent S-023. Initial inspected product: T117
`20403b8d9d89ea7ac19ea935d1948a0c9bf77a3b`. Integrate literal T117 final proof
`d4a4baa1be4e304e40c1001dc1e231e555292d98` before the first Cargo gate.
Status: public behavior contract awaiting independent pre-code review.

## Public sources and independent choices

Sources inspected 2026-10-11:

1. [KiCad 9 PCB Editor: net controls and design rules checker](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html).
   The public manual separates ratsnest visibility from electrical connectivity,
   describes unconnected items and reports unconnected pads independently from
   other design violations. Its track-end diagnostic names pads, other tracks,
   vias and zones as copper connections. This motivates inspecting copper rather
   than counting drawn guide lines. Viamind does not reproduce KiCad's complete
   connection engine or claim its supported object coverage.
2. [MCP 2025-11-25 Tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
   Observations return structured results; invalid requests and execution limits
   produce explicit errors. Tool descriptions and schemas expose the supported
   behavior. Trusted server admission remains authoritative.

All limits, exact supported geometry, status names and snapshot semantics below
are Viamind design choices. No proprietary implementation, artwork or data is used.

## Purpose and compatibility

Add `query_unrouted_nets`, a strict empty-object observation. One successful call
returns the complete bounded board net inventory and its terminal connectivity
classification; no pagination, output limit parameter or silent truncation.
The caller can identify every row with `unrouted: true` without further calls.
Unknown geometry is reported explicitly and never converted into a routed result.

Reuse the copper-connectivity module's union/component foundation and its actual
pad/track/via model, adding a separate contact analysis. Never infer these results
from `guide_count`, visibility, guide endpoints or a preexisting DRC report.
Existing `query_nets`, `query_connectivity`, endpoint routing, ratsnest suppression,
their conservative coverage labels and existing serialized bytes remain stable.
No persistent model/native/event version or dependency change is required.

This observation concerns connectivity between assigned explicit PCB pad
terminals. It does not establish schematic completeness, resolve missing pad
definitions, detect dangling copper, validate shorts, check DRC/manufacturing
rules, certify physical continuity, or authorize output publication.

## Coherent capture and admission

The result identifies the exact `project_id` and complete `source_cursor`
(`stream_id` and sequence) captured together under the engine lock. A borrowed
preflight precedes all secondary geometry/name copies. Capture only fixed
electrical records, assigned net names, explicit pad IDs/parent IDs and net-rule
keys. No whole Project/Board/Part clone: schematic sheets, package contents,
sourcing/calibration/artwork, designator/value/footprint strings, rule values,
outline geometry, selection, history and saved-project copies are unused.
An invalid or huge unused field must not be copied or validated by this query.

Use one of two nonwaiting process-wide query admission slots. The permit is owned
by the actual worker through capture, analysis and bounded serialization, so an
aborted MCP awaiter or closed desktop dialog cannot free a running slot. Reject
busy admission immediately; do not queue unlimited blocking jobs. Capture has
bounded linear work under the lock; spatial graph work and serialization run
outside it. All early errors release their permit. A fallible named desktop
thread has one retained handle/result slot, polls while its dialog is hidden,
and never reports busy after a spawn failure.

Borrowed limits, checked arithmetic and deterministic refusal:

| Quantity | Limit |
| --- | --- |
| Part containers + explicit pads + tracks + vias + net-rule rows | 100,000 |
| Distinct case-sensitive assigned/defined net names | 10,000 |
| Each relevant net name | trimmed, nonempty, no controls, at most 128 Unicode scalar values and 512 UTF-8 bytes |
| Aggregate relevant net-name bytes, including repeated object assignments | 2 MiB |
| Supported transformed coordinates | each axis within +/-1,000 mm |
| Supported copper width/height/diameter/drill | positive, at most 100 mm; via drill strictly smaller than diameter |
| Broad-phase contact and hole candidate visits combined | 1,000,000 |
| Complete serialized result | 8 MiB |

Check cheap container lengths before visiting records, then pad and text totals
before allocating geometry. Use checked additions. Detect nil/mismatched record
keys and duplicate pad IDs before graph construction; refuse malformed identities
or relevant net names. Scalar geometry outside the supported subset becomes
explicit unknown coverage for its named net, rather than allocating huge bounds.
Count/net/text/candidate/output budget exhaustion refuses the whole call with a
specific error; it must not return a partial inventory or pretend a result is
complete. Unassigned objects are counted and considered for drill effects but
do not invent an empty-named net. Exact bounds are stricter than native limits;
not every valid project is accepted.

## Supported copper and exact contact predicates

Analyze the two outer copper sides in the common board coordinate system. Pad
rectangles use the shared exact local-X bottom mirror, quarter rotation and
translation; swap width/height for odd quarter turns. Use doubled integer
coordinates so odd-Nm dimensions retain exact half-Nm boundaries. Checked i128
differences, squares and comparisons implement closed-set contact; no epsilon,
floating distance, rendered pixel, snap or rounding is a connection proof.
Floating bounds may only conservatively select candidates; the final predicates
are integer. Any arithmetic failure refuses the call.

Supported planar shapes:

* Explicit rectangular SMD pads at exact quarter-turn poses, on their mounted side.
* Horizontal or vertical nonzero straight tracks, with their actual round-ended
  width (capsules), on their stored side.
* Circular through-via annuli with actual diameter and open drill, bridging both
  sides. The annulus is one connected conductor; its empty center is not copper.

Same exact net and common side are required for planar joins. Join overlapping
rectangles, actual rectangle/capsule contacts (including a track's interior),
capsule/capsule interior crossings and width contacts, and annulus contacts with
rectangles/capsules/other annuli. Cardinal segment-to-segment and
segment-to-rectangle squared distance compares to the squared summed radii.
For annulus contact, the other connected shape's occupied radial interval must
intersect the annulus interval; use squared comparisons rather than square roots.
Two annuli must overlap their copper rings, not merely their outer disks.
The closed ideal model includes exact boundary contact. One-Nm gaps do not join.
This mathematical convention is not a manufacturing or contact-resistance claim.

Drill removal must never create a false connection:

* All vias, including unassigned and foreign-net vias, participate in hole checks.
* If another via's drill meets a via's outer copper disk, mark affected named
  annuli/net coverage unknown. Do not treat a possibly severed ring as intact.
* A pad/capsule touched by a foreign-net or unassigned drill makes its named net
  unknown; no proof may rely on the original filled shape through that void.
* For an isolated same-net via hole, a convex pad/capsule remains a component
  node only if it has copper outside the open drill and intersects that intact
  same-net annulus. Its possible fragments are then reconnected by that ring.
  A shape wholly drilled away is unknown, not an attached terminal.
* If those conditions cannot be established exactly, mark the relevant net
  unknown. Do not reduce the conditions to a center-only or outer-disk test.

Unsupported arbitrary pad rotations, diagonal tracks, malformed or out-of-range
geometry mark their assigned net unknown, even if another supported path happens
to connect all its terminals. Existing data must not be silently dropped.
Legacy Part.pads=None is counted as an excluded component and makes whole-board
coverage incomplete because its terminal membership is unknown. Explicit empty
pads mean explicitly no represented terminals. Future unmodeled pours, inner
copper, plated pad holes, arcs, blind/buried vias and schematic net obligations
remain disclosed exclusions, not inferred connectivity.

## Whole-query result and meaning

Names are exact/case-sensitive and ordered by UTF-8 lexical order. Inventory is
the union of assigned explicit pad/track/via net names and document net-rule
keys. A rule-only definition is still included with zero terminals. Do not clone
or consume clearance/drill policy values to decide electrical contact.

Return project/cursor, `complete` (no excluded legacy parts or unknown net
coverage), fixed exclusions, supported/unsupported/unassigned object counters,
candidate count and a complete `nets` array. Each row contains its name, assigned
terminal/track/via counts, supported terminal-island count or null, `status`,
`unrouted` boolean or null, and bounded deduplicated coverage reasons:

| Status | Meaning | unrouted |
| --- | --- | --- |
| connected | At least two explicit terminals, all in one supported copper component | false |
| disconnected | At least two explicit terminals occupy multiple supported copper components | true |
| insufficient_terminals | Zero or one explicit terminal; no interterminal routing obligation can be assessed | false |
| unknown | Named net contains unsupported or hole-affected geometry | null |

Unknown takes precedence over all other statuses. Pad-only terminals are valid
isolated nodes. A through via is not an additional terminal. Copper-only islands
without a terminal do not create an interterminal routing obligation; this is
not a dangling-copper check. Multiple pads connected through one physically
represented conductor form one terminal island. Wrong-side or differently named
copper never joins them. Global excluded-part coverage must remain visible even
if all known nets individually classify connected. Consumers must inspect
`complete` and unknown rows before concluding that the whole board is routed.
Return scalar totals for each status and all inventory counts; no cursor for a
second page and no guide/MST computation. Byte-budget refusal occurs before
materializing an unbounded JSON bridge.

## Desktop and MCP parity

Desktop Inspect provides a scrollable Unrouted nets observation dialog using the
same typed request, admitted capture, analysis and bounded result. Show complete
project/full cursor, coverage and known unrouted/unknown totals; list all bounded
rows with their state/island counts/reasons. Display zero/one-terminal meaning and
the distinction from physical continuity/DRC. Keep full result available through
scrolling in a 640x400 viewport. Pending controls disable duplicate starts; hidden
polling, disconnect/spawn failure and retry have readable outcomes. Show when the
current full cursor differs from the captured source; the observation never
changes model, selection, camera, document status, auto-DRC or native file bytes.

MCP exposes the shared typed empty schema and explicit supported-subset description
as an observation in Live, Approval and ReadOnly, including immutable startup
ReadOnly. Future writers keep default admission; this query grants no mutation,
mode change, proposal merge or file-publication ability. Fresh generated reference
and exact real application discovery must include the new tool.

## Required verification before submission

1. Independently reviewed public contract before Rust; record limits and scope.
2. Independently assembled exact geometry: interior cardinal track crossings,
   width-only and pad-edge attachment, annulus-only attachment/side bridge,
   bottom mirrored odd-Nm pad rectangles, open drill false-positive controls,
   nested/overlapping annuli, foreign/unassigned holes, one-Nm gaps, wrong side/net,
   disconnected islands, unsupported diagonal/pose and wholly drilled terminals.
3. Net inventory from pads, tracks, vias and rule-only definitions; zero/one
   terminals, copper-only islands, deterministic complete inventory beyond old
   100/1000 page sizes; no dependence on guide counts or visibility.
4. Cheap count/net/text/coordinate/arithmetic/candidate/output refusal and malformed
   IDs; huge/invalid unused schematic/source/artwork fields excluded before copy;
   retained-worker admission under caller cancellation, busy and failure recovery.
5. Actual production desktop compact input/scroll/hidden polling/stale result,
   all-mode actual initialized two-client MCP result parity and complete immutable
   session/history/status/auto-DRC/native preservation. Retain exact binary/source,
   public reproduction driver, reports, readable native capture and original pixel
   channels with honest local-input provenance.
6. All four Linux workspace gates, actual canonical catalog/reference/workflow
   checks, independent source/evidence review and exact final-head CI. Preserve
   original source-scoped gates through later clean stack integration.
