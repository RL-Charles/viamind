# Explicit circular nonplated footprint holes

Ticket T-0121 / S-035. Public contract for independent review before Rust.
Base `fb81457707ccd734e118e7256d49223539dbabd8` includes reviewed T118/T123.
T120's approved aperture contract and stable document foundation are prerequisites.
Implementation waits for its final command, render, fabrication and release handoff;
only this ticket's commits will transplant onto those frozen interfaces.

## Public references and scope

The [KiCad 9 PCB manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#working-with-pads),
revision `152cd19e`, describes mechanical nonplated holes without electrical connections
and separate plated/nonplated drill output by default. Its oval holes are outside this
increment. [Ucamco XNC 2021.11](https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf),
sections 3.9 and 4.1, defines tool-sized circular hits and file-level plating attributes
requiring separate plated/nonplated files. XNC describes CAD-to-CAM geometry; it does
not supply machine feeds, speeds or manufacturer acceptance. The
[KiCad drill writer reference](https://docs.kicad.org/doxygen/classGENDRILL__WRITER__BASE.html)
also documents the NonPlated/NPTH FileFunction spelling. The bounded model, versions,
commands and compatibility policies below are Viamind choices.

## Stored geometry, identity and bounds

1. Persistent Part gains optional `holes: Option<Vec<PartHole>>`; detached footprint
   definitions gain optional `holes: Option<Vec<FootprintHole>>`. Absent/null outer
   value means Legacy and serializes omitted. `Some([])` means explicitly cleared,
   remains present and survives save/history/library round trips. Both contain no
   physical holes; their authoring intent differs. Old absent-feature bytes stay exact.
2. FootprintHole has required strict fields `key`, `center:{x:Nm,y:Nm}`, `diameter:Nm`.
   PartHole additionally has required nonnil globally unique ObjectId `id`. Keys are
   case-sensitive visible ASCII `[A-Za-z0-9_.-]`, 1..64 bytes, unique within the
   owning footprint. They are mechanical reuse keys, independent of pad numbers.
   Detached files carry no component/global UUIDs. Unknown/duplicate fields reject.
3. Each hole is a circular nonplated through hole, through the complete substrate and
   all copper layers. No side, net, copper annulus, mask/paste shape, pin association,
   per-hole rotation, slot, plated component hole or blind/buried span is inferred.
4. At most 256 holes per Part/definition. Positive diameter is at most 100 mm.
   Checked i128 doubled local extents satisfy `abs(2*center.axis)+diameter <= 2*10000mm`.
   Odd-Nm diameters retain exact half-edge geometry. Reject zero, negative, overflow
   and invalid transformed extents; existing Legacy object limits remain unchanged.
5. Borrowed preflight counts hole rows and key bytes before allocating indexes,
   decoding a secondary row, cloning Parts or creating UUIDs. Hole rows join each
   consumer's existing aggregate object budget; aggregate hole-key text is capped
   at 2 MiB and aggregate holes at 100000. When any Some holes container is present,
   also preflight at most 100000 combined global identity-bearing project objects
   before calling the existing native validator or building its identity set. Legacy
   projects without this feature keep the old native limits. Per-consumer stricter
   budgets remain. Count max+1 takes
   precedence over a poison/null final row. Check global identities before indexes.
6. Shared document geometry resolves local circles to world centers/diameters.
   Existing component bottom mirror, rotation and translation apply exactly once.
   Cardinal rotations/mirrors use checked integer transforms and doubled extents.
   Native/static/fabrication arbitrary milli-degree poses use the existing finite
   transform and nearest-Nm computed center (<=0.5 Nm quantization relative to that
   computed result); no analytic arbitrary-angle exactness is claimed. Exact integer
   routing/continuity accepts only its existing cardinal coordinate subset and limits.

## Persistence, commands and reuse

7. Native minimum feature15 applies to any Some holes container, including empty.
   Compose by maximum with aperture14, artwork13, package12 and via minima10.
   Clearing every container restores the otherwise required minimum. Native event
   base and both before/after Part deltas enforce15; older headers refuse Some holes.
   Event writer maximum advances15. IDs participate in all project-wide collisions.
8. Detached library4 requires at least one present holes container, including empty;
   artwork and aperture policies remain independently optional. Library1..3 forbid
   a holes member including null. Writer chooses4 for Some holes, otherwise preserves
   T120's 3/2/1 selection. Keep existing 1 MiB library, 4096-byte path, same-fd bounded
   regular-file reader, staged NEW/no-clobber and latest-native protection.
9. Shared `set_part_holes` requires project ID, exact expected Part, full initial
   transaction cursor and a present nullable hole list. Null restores Legacy. Rows
   include existing IDs for edits and caller-supplied fresh IDs for additions; reject
   duplicate/nil/colliding IDs before mutation. One atomic Part delta, exact undo/redo,
   no-op produces no event. Raw/typed/batch paths apply identical stale/bound checks.
10. Preserve pads/IDs/numbers/nets/copper/apertures, component pose and all schematic,
    package, assembly and artwork metadata. Existing pad edits retain holes exactly.
    Applying a detached footprint replaces holes authoritatively: matched keys retain
    hole IDs, added keys receive fresh globally checked IDs, removed keys disappear.
    UUID generation has a finite 1024-attempt collision budget and fails atomically.
    Library1..3 explicitly restore Legacy holes. Require fullcursor whenever source
    or target has Some holes, in addition to existing artwork/aperture conditions.
11. Detached editor exposes Legacy/Explicit, bounded stable-key rows, exact decimal
    local centers and diameter; clear-all differs visibly from Legacy. Stable row and
    field IDs survive removal of preceding rows. Compact fixed actions precede scroll
    content. Save NEW/load never implicitly applies. Retained worker disables draft/
    path/reset mutations; Close retains receiver and draft. Actual 640x400 input tests
    verify effective ancestor clips before each action and cover stale/reopen/Undo/Cancel.
12. Guarded application, desktop and MCP share the same typed command/descriptor.
    New observations respect hook reentrancy before worker reservation/engine locks.
    Hole edits use existing Live writer/approval policy and branch command allowlists;
    no unreviewed branch admission is added. Log/replay/native replacement/expected
    snapshots and BorrowedPart byte accounting retain exact presence, IDs and key order.

## Physical consumers and conservative electrical claims

13. Every DRC, routing, walkaround, via, cleanup and continuity purpose snapshot retains
    physical holes even when it omits unrelated artwork/apertures. Charge them before
    copying. No electrical analysis shell may silently remove a physical drill.
14. DRC applies existing hole-to-hole, hole-to-copper and board-edge clearances to NPTH
    against all vias, holes, pads and tracks on every layer, independent of net/side.
    NPTH has no owned copper to exempt. Stable reference includes Part/hole UUID/key.
    Existing pair/shape/violation bounds include holes and truncation remains explicit.
    Keep existing floating DRC tolerance disclosed. Positive model diameter is a
    validity check; this increment adds no manufacturer minimum NPTH diameter rule,
    and minimum_via_drill/annular/named-net minima continue to apply only to vias.
15. Guarded routing and copper edits run complete shared scoped DRC with NPTH geometry.
    Direct/cardinal/polyline/via paths and static walkaround cannot cross a drill merely
    because its Part is on the opposite side or its net is absent. Walkaround uses
    conservative integer boxes expanded by route half-width + hole-to-copper clearance
    +1 Nm and shares existing obstacle/work limits. Unsupported/unlocated physical
    holes or exceeded admission budgets refuse before live mutation, without partial route.
16. CopperProof and query_unrouted_nets include NPTH as subtractive drill obstacles,
    never as terminals, net members, annuli or layer bridges. Existing supported exact
    proof may establish continuity only through intact copper. Where hole/copper
    intersection is not representable in that subset, affected net is unknown with an
    explicit physical-hole reason; never connected/disconnected from ignored geometry.
    Unlocated or unsupported holes prevent a complete result. Preserve whole-project
    cursor coherence, worker retention, two-slot admission and existing output limits.
17. Native and SVG/PNG draw resolved circular drill geometry with no invented copper
    or solder openings. Physical hole role/UUID/key is explicit in static metadata.
    Bounds/Fit/placement ghosts and selected emitted primitive budgets include holes.
    Native display exposes one mechanical-hole visibility choice across both sides;
    holes have parent-Part selection semantics and no electrical snapping/ratsnest.
    No-hole default scene/SVG/PNG bytes remain exact. Visible NPTH markers are geometry
    previews, not a claim of boolean copper clipping or manufacturing verification.

## Fabrication, saved jobs and independent readers

18. Standalone fabrication adds default-false `include_npth`. False preserves legacy
    no-hole files/bytes and refuses any Some holes container, including empty, so
    explicit intent is never silently dropped. True emits deterministic fixed
    `nonplated-through.xnc`, including a valid empty file, independently of plated vias.
    Preserve every existing plated-through/drill/copper/profile file byte for otherwise
    identical input. Share original copper coordinate axes/origin; no drill-only offset.
19. NPTH file uses metric decimal circular drill hits, exact Nm tool diameters/centers,
    stable diameter then Part/hole UUID ordering, and
    `; #@! TF.FileFunction,NonPlated,1,2,NPTH` plus matching SameCoordinates metadata.
    No mixed plating, slots/rout commands or machine instructions. At most 99 distinct
    tool diameters per file (XNC T01..T99); preflight max+1 before publication.
    Conditional summary records authored containers, explicit-empty Parts and actual
    physical drill count; absent feature/flag adds no metadata. Same snapshot and
    bounded bytes/permit/cursor feed drill, copper, assembly and optional DRC outputs.
20. Saved job6 requires `include_npth:true`. Jobs1..5 forbid that field including
    false/null and refuse Some holes before publication. Job6 permits absent or true
    include_paste and include_silkscreen independently; false/null refuse. If paste is
    requested, retain job5's complete explicit policy requirement (`pads:None` rejects,
    known empty pads allowed). Without paste, reject every Some aperture container just
    as jobs1..4 do. Existing calibrated/populated relationships remain exact. Writer
    chooses6 for NPTH, otherwise retains T120's 5/4/3/2/1 choices. Job6 does not assert
    that all absent-hole components have been inspected for mechanical intent.
21. Exact release file matrix adds one NPTH file to the independently chosen paste,
    Legend, calibrated/populated and checked-profile companions. Increase strict maximum
    file count only by that one feature; preserve individual/total/read/command budgets.
    Validate all version/flag/presence relationships before staged NEW directory creation.
22. Independent fabrication/release readers parse NPTH file attributes, same coordinates,
    unique tool declarations, bounded drill hits and diameters and require expected
    file/count/plating consistency. Wrong plating/span/coordinates, undefined tools,
    duplicates, mixed/unexpected files or unsupported commands refuse read-only.
    Overlay draws parsed unplated circles independently of model resolver. Compare
    original fixture coordinates/diameters with parsed output on cardinal/mirrored/bottom
    and arbitrary supported I/O poses. Inspector verifies observed release consistency,
    not source authentication, complete intent, drill suitability or manufacturer DFM.

## Acceptance before merge

- Exact absent-feature native/log/library/default render/fab/job/release and electrical
  bytes; Some-empty intent; 15/14/13/12/10 maximum composition and clearing; old-header
  base/both-delta rejection; library4 presence and old-library forbidden-member rules.
- Stable keyed reuse preserves pads/nets and holes, bounded UUID collision failure,
  global identity conflicts, poison-row precedence, aggregate bounds, odd-Nm/cardinal
  integer oracles, arbitrary I/O quantization, exact undo/redo/no-op/session preservation.
- Actual compact/wide native editing, detached NEW/load/apply, two-client initialized
  MCP and CLI guards including peer edit/reopen; retained source/binary/framebuffer proof.
- Opposite-side/unassigned physical holes constrain every routing/DRC/continuity path;
  overlapping-hole/copper negative fixtures cannot falsely connect; no false coverage.
- Independent NPTH XNC/overlay agreement, empty and 99/100-tool bounds, unchanged plated
  files, exact job6 companion matrices and paste-policy composition, malformed reader
  refusal and no-staging atomic failures. One shared output snapshot/permit/cursor.
- Four Linux gates, actual generated catalog/workflow, independent review, exact-head
  CI and retained artifact hashes. No human Done transition is performed by an agent.
