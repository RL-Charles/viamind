# Persisted exact named-net copper minima (T-0097)

## Public behavioral sources and independent scope

- [KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html),
  Board Setup → Net Classes, Custom Design Rules → Constraint Clauses,
  and Design Rule Checking → Track width, accessed 2026-10-11. Public behavior:
  copper clearance concerns different nets; minimum track width is an enforced
  constraint, while a preferred/net-class track width is a distinct routing
  choice. A stronger minimum is not weakened by a smaller preferred setting.

Implement this exact subset from public behavior and Viamind's own model and
transaction contracts. No proprietary code/assets, copied custom-rule language,
net-class implementation, pattern assignment or preferred-size inference.

## Persisted model and finite validation

Add an optional/omitted-if-empty deterministic ordered exact-net map to Board.
Each row contains integer-Nm `minimum_copper_clearance` (0..100mm inclusive)
and `minimum_track_width` (1Nm..100mm inclusive). Net names must be nonempty,
trimmed, control-free, at most128 Unicode characters/256 UTF-8 bytes, matching
existing route labels exactly. No normalization, wildcard, inheritance, net
creation, automatic assignment or layer-dependent rule resolution.

At most128 rows and32768 aggregate net-label bytes are accepted. Check map/row
count before iteration/allocation; validate each net/value and aggregate text
before a capture or owned copy. Shared validators serve native load/save,
commands, direct in-memory DRC and captured plans. Typed invalid in-memory
rules must refuse rather than be silently ignored. Distinct names stay exact;
duplicate editor/request rows refuse before collection rather than last-wins.

A completely empty map keeps old native minimum versions and exact serialized
bytes, including field ordering/absence. Nonempty rules require native schema8
(or the next unoccupied version at implementation), and older envelopes must
refuse nonempty rule data. Save/open preserves odd-Nm values and exact names.
Clearing the map restores the old minimum feature-version calculation.

## Shared rule resolution and actual DRC coverage

For different-net/unassigned copper A and B on a shared supported layer:

`required = max(request copper baseline, minimum[A.net], minimum[B.net])`.

An unassigned or unmatched object contributes zero document minimum. Same-net
copper keeps existing contact semantics. Persisted minima cannot be weakened by
an explicit per-request zero or smaller value. Edge, substrate, hole spacing,
hole-to-copper, annular and via drill rules keep their current request semantics;
this increment adds no document policy for those quantities.

Assigned stored tracks whose exact integer width is less than their matching
minimum produce a new scoped `minimum_track_width` violation, including actual
stored width, required width and stable actual Track reference. Equality passes.
No named minimum applies to an unmatched/unassigned track. No stored geometry
is automatically widened or moved. Existing exclusions and incomplete/report
limits remain authoritative; complete scoped DRC is not manufacturing or
physical-connectivity certification.

The copper broadphase must inflate by the maximum of the existing global search
requirements AND every persisted copper minimum before pair enumeration. Keep
existing candidate/work/result caps and mark incomplete exhaustion honestly.
A far foreign pair that is outside baseline clearance but inside either net's
larger document minimum must be discovered. Per-pair narrowphase uses the exact
resolved requirement, not the global largest named minimum. Width checks share
bounded result accounting and deterministic stable-object ordering.

Return bounded document-rule provenance alongside the existing request rules:
source project/cursor remains supplied by existing capture wrapper, actual
request baseline remains visible, and document exact-net rows/minima are
identified separately. Omit new report provenance when the document map is
empty so preexisting empty-rule report bytes remain unchanged. Do not claim
that the request baseline alone is the effective requirement for every pair.
Desktop batch/automatic DRC and routing coverage must identify persisted minima
when present; displayed rows are bounded and request values remain explicit.
Checked same-side/two-side walkaround and cleanup query outputs may add bounded
optional document-rule provenance, omitted when empty so old output bytes stay
unchanged. No second project capture is needed to obtain these rows.

## Immutable capture, planning and guarded route parity

All scoped DRC, automatic DRC, free/pad/posture/through-via routing, same-side
walkaround, fixed-via two-side walkaround and cardinal cleanup candidates carry
the exact captured document rules. Check rule counts/labels/values before any
board/rule clone; add bounded copied net-label bytes/records to existing shared
capture budgets. Continue excluding unrelated schematics and unused assembly
metadata. Audit each Board::clone_without_assembly caller, including render and
fabrication snapshots, so newly copied policy cannot escape a caller's borrowed
bounds. Read-only rendering may omit unused policy explicitly; checked output
must retain it with provenance. Do not introduce an engine→render dependency.

All supported guarded routing APIs continue to require explicit width. Reject
below-minimum width early, before segment allocation/search/DRC where practical,
and still enforce it in final shared joint DRC. Never silently widen a requested
route. For walkaround obstacle inflation use the maximum of the request baseline,
routing net's minimum and each foreign obstacle net's minimum, plus the existing
route/obstacle half widths and strict1Nm margin. Preserve current exact Nm
rounding, hole/cutout rules, supported rectangular/quarter-turn scope, combined
work/segment/obstacle limits and one admission/captured snapshot for two legs.
A checked preview must not promise a path that its same-snapshot joint DRC
rejects under those minima; guarded commit recomputes against the same full
initial transaction cursor and all current document rules before mutation.

Old route command schemas and default widths/geometry remain unchanged. Empty
rules preserve exact old routes, guide behavior, candidate ordering, DRC results
and limits. Rule edits invalidate retained route/cleanup/automatic reports via
existing full cursor/design revision rather than publishing stale checked plans.
Saved checked-output requests resolve their baselines against the captured
document minima centrally and report both sources; coordinate this contract
with the checked-output-profile lane before implementation.

## Shared commands, MCP and desktop editor

Provide immutable `query_named_net_rules` returning bounded validated rows plus
project/full cursor, and guarded `set_named_net_rules` replacing the bounded map.
The typed/dynamic/batch command requires expected project and full initial
cursor, denies unknown fields and emits one undoable rules delta only after
complete input validation. One undo/redo restores exact map/old schema behavior;
no-op emits no design event. Stale sequence, same-project reopen/new stream,
duplicate name, oversized input or invalid value rejects all edits without
changing project/cursor/selection/file/undo state. Grouped commands retain the
engine's one captured initial transaction cursor contract.

A native Board menu editor loads the immutable captured rows into local fields,
allows adding/removing exact names and explicit integer-Nm minima through exact
millimetre parsing, then atomically applies all rows through the shared command.
Existing incompatible local drafts block opening; Cancel/Escape changes nothing.
Peer edits/reopen invalidate the editor, with clear refresh/restart behavior.
Use a bounded scrolling table and persistent Apply/Cancel controls visible at
640×400. Expose all actions through matching engine/MCP commands. Explain these
are enforced minima and routing width remains explicit; no automatic widening.
Setting valid minima may reveal existing violations; report this without silently
moving copper or refusing merely because existing geometry violates new policy.

## Required verification and honest limits

- Exact odd-Nm rule roundtrip, empty-map legacy serialized byte/version parity,
  nonempty schema enforcement, all count/text/value/duplicate validation bounds.
- Independently expected pair max for each side, unmatched/unassigned and same
  net cases; below/equal/above integer width; request cannot weaken document.
- Distant pair broadphase regression and bounded/incomplete candidate accounting.
- Empty-rule DRC report/route geometry parity against independent old fixtures.
- All routing families plus cleanup/autoDRC snapshots consume minima; unsupported
  scenes and complete joint-Drc refusal stay atomic. Two-side planner preserves
  one board/cursor/admission and aggregate counters. Large unused sourcing and
  unrelated schematic content remain uncopied.
- Typed/dynamic/batch stale stream/sequence, same-ID reopen, whole-map oneundo,
  no-op and malformed/over-budget unchanged-state tests; two independent actual
  MCP clients discover/query/edit/undo and observe identical rule sources.
- Actual native egui menu/local edits/Apply/Cancel, peer invalidation and640×400
  footer bounds; exact production HTTP checks and readable native renderer proof.
- Regenerate the canonical full-app MCP tool catalog when T96 lands; keep its
  deterministic schema/description drift gate current after both new tools.
- Required Linux fmt, strict all-target workspace Clippy, tests and build under
  shared whole-sequence mutex; unique binary copies and durable provenance.

No class hierarchy, patterns, optimal/preferred width, implicit widening,
general-shape push/shove, differential pairs, length matching, impedance/current
calculation, full manufacturing certification or new strict-frame-rate claim.

This is an initial S-055 increment consumed by S-059 routing. Story acceptance
for net classes, via minima, per-region priority rules and shove remains open.
