# Persisted exact named-net via minima (T-0101)

## Public source and independent subset

[KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html),
Custom Design Rules → Constraint Clauses, accessed 2026-10-11: a minimum
constraint is enforced by DRC, while optimal sizing is a separate choice;
annular-width and drilled-hole-size constraints concern different measurements.
Implement independent bounded exact-name floors for Viamind's circular
through-via model only. No copied rule language, assets, class hierarchy,
proprietary implementation or broader drilled-pad geometry.

## Model, canonical omission and feature versions

Extend the existing exact named-net record with integer-Nm
`minimum_via_drill` and `minimum_annular_width`, each defaulting to zero and
validated within 0..100mm inclusive. Zero disables that document floor; the
existing request baseline still applies. Explicit route via diameter/drill
remain mandatory choices; never infer preferred geometry or silently resize.

Canonical model/native/report/delta/command serialization omits each zero
field. Missing input means zero; guarded request rows may explicitly supply
zero to clear it. Both new fields zero must yield byte-identical serialized
records to the pre-extension implementation, including field ordering. Existing
clearance/track-width records remain native8; empty maps preserve older feature
versions as before. Any nonzero via floor requires native/event feature10.
Compose the document minimum version as the maximum of present independent
features, including assembly9; clearing via floors returns to the applicable
older feature version without deleting other policy or assembly metadata.

Old native envelopes must refuse nonzero via floors before returning a project.
Event replay refuses new nonzero floors below version10, whether they appear
in the base project or policy before/after maps. Both-zero policy deltas retain
version8 compatibility. New global log writer may use highest supported10;
loader/replay still accepts legal historical8/9 logs. Full-session and policy
provenance custom bounded deserialization continues to reject duplicate names.
No zero-valued new keys are emitted while labeling serialized data schema8/9.

Retain existing128-row/32768-label-byte/name/range rules. Borrowed policy
validation checks all new scalar values before copying maps or geometry.
No additional allocation/search dimension is introduced; every prior capture
consumer continues to validate/charge the same map rows and labels.

## Central DRC and exact comparisons

For an assigned circular through via:

- required drill = max(request minimum drill, matching exact-name drill floor)
- required annular width = max(request minimum annular width, matching
  exact-name annular floor)

Unassigned/unmatched vias contribute zero document floor. All-net hole spacing,
hole-to-copper, edge clearance and copper rules keep their current semantics.
The central resolver lives with the shared document policy and is used by the
existing scoped DRC, never reimplemented in saved-output helpers.

Drill comparison uses exact stored integer diameter. Annular width is half the
integer difference between outer diameter and drill. Compare the doubled
annulus `diameter - drill` against `2 * required_annular_width` using checked
integer arithmetic, avoiding rounding that changes equality. Preserve current
public measured-clearance representation at the reporting boundary; an odd
one-Nm diameter difference must not silently round into a different decision.
Keep deterministic stable Via IDs, actual required threshold and captured
request/document provenance. Existing result/candidate/coverage limits remain.
Empty new minima preserve prior DRC bytes, ordering and geometry decisions.

## Guarded routing and checked manufacturing outputs

All explicit-via routes, through-via posture and fixed-via walkaround validate
specified drill and annulus against the same current captured net floors early,
before search/segment/candidate allocation where possible, then apply complete
shared joint DRC to all generated tracks plus via before any mutation. Preserve
existing request schemas except optional policy editor row extensions. No
unrequested geometry changes, extra vias, net inference or route widening.

Every checked plan retains the same exact immutable Board/full cursor/policy.
Below-minimum explicit geometry refuses atomically in both plan and commit;
equality passes if other scoped checks pass. One undo restores all generated
tracks/via and policy remains unchanged. Existing same-side/no-via routes are
unchanged by via-only minima. Policy edits invalidate old checked plans by the
existing full initial cursor contract.

Saved checked output jobs run the same central DRC on their own captured board.
Zero/smaller request drill/annulus baselines cannot weaken matching document
floors. Validation reports distinguish unchanged requested Rules and exact
captured document_net_rules. A refusal publishes no output or staging tree;
existing generic outputs remain unchanged when no new floors apply.

## Shared editing, UI and agent parity

Extend existing query/set policy rows rather than introduce another policy
store or separate mutation mechanism. Full-cursor whole-map guarded edits,
no-op, oneundo/redo, stale/reopen/batch behavior and duplicate rejection remain.
The native editor adds exact-decimal via-drill and annular-width fields per row,
explains zero means no document floor and explicit via sizing remains a user
choice, retains stable field identities and640x400 Apply/Reload/Cancel bounds.
Cancel stays local. Existing unused-policy byte behavior must remain intact.
MCP canonical schemas/descriptions are regenerated from the verified full app;
read-only query classification stays observation, setter stays blocked in
startup agent read-only mode.

## Required evidence and limits

- Nonzero native10/replay gating, mixed assembly9→max10, clear→9/8/legacy,
  zero/absent exact old native/report/delta bytes, duplicate/bounded values.
- Independent below/equal/above drill and annulus cases, both sides of odd-Nm
  annular comparison, unmatched/unassigned/net-case behavior and request max.
- Actual automatic DRC refresh; free/manual/posture/fixed-via shared minima,
  atomic stale-peer/reopen/undo and retained one-board snapshot policy.
- Saved zero-baseline checked release equality and document-floor refusal,
  captured provenance, no publication/staging and complete session invariance.
- Actual compact native editor input and local Cancel, footer/row-identity
  assertions; two-client production MCP plus readable native renderer capture.
- Linux fmt/strictalltargetClippy/workspace tests/build, full-app catalog drift,
  durable exact source/binary/fixture/report hashes, final combined CI.

This is circular through-via minimum sizing only. No classes, patterns,
layer/region priorities, drilled pads, micro/buried/blind vias, preferred sizes,
implicit resizing, impedance, push/shove or physical/manufacturing certification.
Broader S-055/S-059 and native100k60fps acceptance remains open.
