# Guarded equivalent cardinal track cleanup (T91)

## Public behavior and independent scope

The public [KiCad 9 PCB editor manual, Cleaning up tracks and vias](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#cleaning-up-tracks-and-vias)
describes combining connected collinear tracks into an equivalent single track,
filtering cleanup by net, layer or selection, and inspecting proposed changes
before explicitly applying them. Those observable behaviors inform this original
increment. No external implementation, layout, artwork or algorithm is copied.

Viamind will combine redundant **cardinal straight joints** while preserving the
exact union of round-ended copper segments. This is a deliberately restricted
first child of S-063. It does not change angled bends, optimize route length,
remove overlapping/retraced/dangling copper, remove vias, shove obstacles or
claim physical connectivity certification. Existing routing and guide APIs keep
their signatures and behavior.

## Exact supported subset

Two or more stored straight tracks can become one track only when all have the
same nonempty assigned net, copper side and exact integer-Nm width. Each track
must validate and be horizontal or vertical. Adjacent tracks meet at exactly
equal stored endpoints. At each removed joint, the other endpoints lie strictly
on opposite sides along the same axis: no overlap, zero-length result or reversal
is allowed. No snapping, rounding, coordinate movement or width change occurs.
Integer comparisons determine all eligibility; floating-point tolerances are
not used to decide equivalence. The union of the original equal-width round
capsules then equals the single replacement capsule exactly.

An internal joint must have exactly two incident stored track endpoints on its
copper side, counting tracks outside the requested selection and all nets. Keep
every branch, explicit pad center and through-via center. All explicit pad
centers are resolved with the existing exact quarter-turn/mirror transform;
unsupported or overflowing explicit-pad transforms reject the operation rather
than guessing which joints are safe. Legacy parts without explicit pads retain
their disclosed existing DRC exclusions. Opposite-side tracks alone do not form
a junction, while through vias anchor both sides.

To preserve the existing conservative stored-endpoint guide proof, also exclude
any candidate chain whose closed centerline AABB meets the closed drill-hole
AABB of ANY stored via, regardless of net. Use the existing conservative radius
`ceil(drill/2)` in integer Nm; contact counts as intersection. This deliberately
retains some otherwise redundant joints near holes. Cardinal merging keeps the
centerline AABB union unchanged, unlike diagonal merging, but growing a single
incident segment can otherwise change the proof's annulus-reach condition.
Invalid via geometry rejects analysis. The proof implementation is unchanged.

Analyze the original board deterministically. Collapse eligible maximal chains
between retained endpoints. The smallest stable track UUID in each chain is the
survivor; its geometry spans the two retained far endpoints in deterministic
coordinate order. Other chain IDs are deleted. Preserve all other objects and
attributes. Do not create a replacement identical to an existing stored track
outside the chain; report that chain as unsupported. Independent chains are
ordered by survivor ID. A preview must report unsupported diagonal tracks and
retained anchor/hole/duplicate joints, so a partial supported subset is explicit.

## Shared query, guard and bounded execution

Add immutable `plan_cardinal_track_cleanup` and typed shared command
`cleanup_cardinal_tracks`. Inputs deny unknown fields and contain expected
project UUID, full initial session cursor, the existing DRC rules, and exactly
one selector: an explicit stable list of track IDs, or an exact nonempty assigned
net plus optional outer copper layer. An explicit list rejects missing IDs,
duplicates and non-track IDs; it never silently uses current transient selection.
A net selector captures its matching original track IDs before planning.

Both paths acquire one nonwaiting admission from a dedicated shared limit of
**two cleanup operations** before board capture. A retained worker holds its
admission until completion even after cancellation. Capture one bounded
geometry-only board under the engine lock and its full source cursor; never
clone unrelated schematic contents or unused assembly metadata. Run immutable
query planning and DRC outside the engine lock. Commit validates its expected
initial transaction cursor and recomputes against the transaction's current
board; only complete success may apply deltas.

Before cloning or building indexes, enforce the existing borrowed scoped-DRC
capture budget, capped at **100000 structural objects, 512 bytes per indexed
label and 2 MiB aggregate copied board text**. Track selectors are capped at
**2048 IDs/matches**, checked before collecting their owned IDs. At most
**1024 deleted segments** may be proposed, and at most **2048 original selected
segments** can participate. Refuse an over-budget operation without truncating
the requested selector or publishing a partial plan.

Endpoint and anchor indexes have at most two entries per captured track/via plus
one per explicit pad, bounded by the structural cap. Use deterministic
ordered indexes and chain traversal, with no all-pairs scan. Hole broad-phase
queries share a maximum of **1000000 visited candidates** for the complete
operation; charge before inspecting each candidate. Overflow or exhaustion
rejects the entire operation. Candidate board copying and result arrays remain
bounded by the same capture and selector limits. Existing DRC pair/result caps
remain authoritative. Return useful selected/analyzed/retained/unsupported/
merged/deleted counts and bounded proposed before/after records.

## Joint DRC and transaction behavior

Build one candidate board containing EVERY proposed replacement and deletion.
Require an explicit valid substrate outline and run the existing complete scoped
DRC jointly against this candidate with the supplied rules. Reject any violation
involving a replacement survivor, and reject any truncated or incomplete report.
Existing unrelated violations may remain; existing checker exclusions are
reported rather than presented as an all-clear manufacturing certificate.
No success result may escape before this shared candidate check completes.

The immutable preview leaves the complete session, project, selection, dirty
state and undo history unchanged. Apply recomputes the same supported changes
under its full cursor guard, then emits one atomic transaction. One undo restores
all original IDs/geometries; redo repeats the identical survivor/deletion set.
Deleted IDs follow existing selection-pruning behavior; design undo does not
promise to restore transient selection. A no-op returns an empty successful plan
and emits no design event. Failure, stale cursor, same-project reopen, admission
refusal or incomplete validation leaves all state unchanged. Typed, dynamic and
grouped commands preserve the engine's existing initial-transaction cursor rule.

## Desktop and agent parity

Provide an explicit board cleanup dialog with source choices for selected tracks
or an exact named net, plus optional layer filtering for the net source. Existing
canvas track picking remains usable. Display the captured selected track count,
the supported subset, retained/unsupported counts and rule settings. Mixed
part/via selection is described and filtered to tracks in the UI before creating
the explicit shared request; no selected tracks is an actionable error.

An explicit Preview action uses one retained worker. Show proposed surviving
segments/deleted joints as local preview with no project mutation, and expose
Apply only for a current complete checked plan with at least one deletion.
Apply invokes the same recomputing engine command advertised through MCP.
Poll the retained worker every frame even when the dialog is hidden. Full cursor,
request and generation must still match before publishing. Source selection,
net/layer/rules edits, cancellation, document lifecycle changes and stale peer
edits clear the preview; late completions cannot revive it. Preserve existing
routing, schematic wire, placement, file and modal draft guards. Cancel/Escape
clear local preview without dispatching an edit.

## Required evidence and honest limits

Root reviews this spec before implementation. Tests independently expect exact
horizontal/vertical chains in both signs and copper sides, odd Nm widths,
deterministic survivor IDs, separate chains, selected/net filters, diagonal and
width/net mismatch retention, branches, quarter-turn mirrored pad anchors,
through-via anchors and near-hole proof preservation. Compare guide endpoints
before/after successful cleanup. Cover invalid/unsupported geometry, duplicate
replacement, selector/capture/merge/hole limits before excess allocation/work,
two-operation admission, complete candidate DRC failures/incomplete reports,
huge unrelated schematic data, immutable queries, two-client stale/reopen guards,
grouped rollback, no-op event behavior and stable atomic undo/redo.

Actual desktop input tests cover track/source selection, explicit immutable
preview, usable controls, cancellation, request changes, stale worker results,
atomic Apply and undo. Retain actual production two-client MCP verification,
all observed latency samples and a readable native checked-preview capture with
exact source/binary/fixture provenance. Linux fmt, strict workspace all-target
Clippy, workspace tests/build and final combined CI are required.

This increment removes redundant cardinal straight joints only. It leaves
diagonals and conservatively excluded hole/anchor cases in place. It does not
complete general gloss/retrace optimization or physical/manufacturing checking.
The strict native100k60fps criterion remains open. Root reviews/merges; no
self-merge or ticket done transition.
