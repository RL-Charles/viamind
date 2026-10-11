# Explicit rectangular SMD mask and paste apertures

Ticket T-0120 / S-035. Public behavioral contract before implementation.
Initial worktree base: verified T-0122 `1e113aa17e158510b844a4a23d1e8cace4ae1b6f`.
T-0119 is a required predecessor: only this ticket's commits will transplant onto
its final frozen interfaces before implementation. Research reads immutable T119
`f738e00a2e8eff868c9f7056c69b6eef901c8596` and `c3b9d3a` consumer contracts;
no changes to that lane or uncommitted structures are assumed.

## Public sources and independent choices

The [KiCad 9 PCB manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#working-with-pads),
revision `152cd19e`, describes mask/paste openings, intentional removal of paste,
and signed absolute expansion: an equal border changes total width by twice its
margin. It also describes relative margins and several override levels. This
increment uses explicit absolute rectangles instead of reproducing that hierarchy.
The [Ucamco Gerber 2026.05 specification](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf),
sections 5.6.3–5.6.5, distinguishes mask openings from paste deposition, file
polarity, and a shared coordinate system. Mask openings use negative file polarity;
paste uses positive polarity. All limits and version policies below are Viamind
choices. No proprietary assets, code or binary inspection is used.

## Stored policy and absence compatibility

1. Add optional `apertures: Option<PadApertures>` to persistent `Pad` and detached
   `FootprintPad`. Absent or null outer value is **Legacy** and serializes omitted.
   Existing native/library/command/default scene/export bytes remain exact when
   the feature is absent. No new UUID is allocated for aperture shapes.
2. `PadApertures` has two explicitly required nullable fields: `mask` and `paste`.
   Each non-null value is a strict rectangle `{width: Nm, height: Nm}`. Missing,
   duplicate or unknown inner fields reject. `Some {mask:null,paste:null}` means
   intentional omission of both shapes; it is meaningful, persists and differs
   from Legacy. One omitted shape never means inherit the copper dimensions.
3. Every rectangle is centered at its owning pad's original local center, uses the
   same local axes and belongs to the component's mounted side. No separate offset,
   angle, opposite-side layer, rounded shape, polygon, stencil island, percent rule,
   via tenting or manufacturer/mounting/default inference is introduced.
4. Dimensions are positive integer Nm through 10000 mm. Check doubled integer
   extents `abs(2*center)+dimension <= 2*10000mm` on each axis before copies/output;
   this preserves odd-Nm half-edge geometry without integer truncation. Zero,
   negative, overflow, out-of-range or collapsed rectangles reject atomically.
   Legacy pads keep their existing limits unchanged.
5. A shared document resolver preserves provenance: Legacy, explicit rectangle, or
   intentional omission for each shape. Every explicit consumer uses its exact
   dimensions. Existing Legacy desktop mask preview remains copper plus 0.05 mm
   on each side; Legacy fabrication remains the request's nonnegative expansion.
   These inherited Legacy policies are deliberately disclosed, not presented as
   equivalent manufacturing geometry. Legacy paste is unconfigured, never inferred.
6. The detached editor uses exact decimal absolute width/height fields and separate
   omit controls. A bounded signed equal-margin calculator produces explicit dimensions
   by checked `copper_dimension + 2*margin`; persisted data remains absolute Nm.
   Invalid calculator input/collapse cannot partially alter a draft or live model.

## Native, detached files and history

7. Native minimum feature **14** applies iff any pad has a non-null aperture container,
   including both omitted shapes. Compose by maximum with artwork13, package12,
   via minima10 and older features. Clearing all containers lowers the minimum when
   no other feature needs14. Validate event base and both before/after Part deltas;
   older headers carrying Some apertures reject. Event writer maximum advances14.
8. Detached footprint library **3** requires at least one non-null aperture container;
   artwork remains independently optional. Library1/2 refuse any pad aperture member,
   including null. Library1 still requires absent artwork; library2 still requires
   non-null artwork. Library3 without Some aperture policy rejects instead of silently
   downgrading. Writer chooses3 for Some apertures, otherwise2 for Some artwork,
   otherwise1. Existing library1/2 byte representation remains exact.
9. Save NEW/load/export retain existing 1 MiB file, 4096-byte path, bounded text/count,
   same-fd NONBLOCK regular-file source reader (existing symlink policy preserved),
   and staged no-clobber/latest-native lifecycle
   guards. This feature does not add a global native-project file cap. Initial transport
   JSON ownership is not described as allocation-free or universally bounded.

## Commands, authoring and guarded reuse

10. New shared `set_pad_apertures` identifies project, part and existing pad by UUID,
    requires exact expected Part and full initial transaction cursor, and requires
    a present nullable requested aperture policy. Missing request is invalid; null
    explicitly restores Legacy. Raw/typed/batch execution rejects stale part/project/
    stream/sequence before mutation. One Part delta/undo; no-op produces no event.
11. Preserve pad IDs/numbers/copper geometry/nets, component pose, schematic symbol or
    package links, assembly fields and artwork. Existing `set_part_pads` preserves
    aperture policy by retained pad ID when editing/renaming/moving/resizing that pad;
    new pads receive Legacy. Revalidate retained explicit local extents after moves.
    Copper size changes do not silently resize absolute aperture geometry.
12. Detached authoring edits aperture choices independently of a selected PCB component.
    Stable editor/pad/field IDs survive earlier row removal and virtualization. Legacy
    and Explicit modes, mask/paste omission and absolute dimensions are visibly distinct.
    Reset/edit/path mutations are disabled while a retained worker runs; Close retains
    receiver/draft. No completed load/save automatically applies a footprint.
13. Applying a definition authoritatively replaces aperture policy. Matching pad numbers
    retain pad UUIDs/nets; other part metadata remains exact. Applying library1/2 restores
    Legacy explicitly. Extend optional fullcursor guards so a source or target containing
    Some apertures requires fullcursor, in addition to existing artwork requirements.
    Existing Legacy-to-Legacy omitted-cursor calls keep their old behavior.
14. Desktop uses the same typed command and MCP descriptor. Persistent compact footer
    actions precede bounded scrollable rows. Actual 640x400 input tests must reveal and
    validate effective ancestor clip before interacting with each aperture field/action,
    retain focused text state through row removal, and cover stale/reopen/Undo/Cancel.

## Borrowed bounds and copy audit

15. Keep 1024 pads/part and existing string/aggregate/path limits. Charge one structural
    row for each Some container and each non-null rectangle, before validation/index/
    clone; at most three new records per pad. Emitted primitive budgets independently
    count actual copper, resolved mask and requested paste shapes before allocation.
16. Raw writer/setter/apply preflight checks pad count before decoding another row,
    then known text/path/aggregate limits and aperture structure before typed secondary
    allocation. Oversized max+1 input with a null poison row must return the count error
    first. Strict finite fixed-size aperture fields cannot hide arbitrary child subtrees.
17. Audit native validation/save, delta serialization/replay, selected/placement snapshots,
    guarded expected Parts, detached export/application, render/fab snapshots and every
    Pad/FootprintPad literal. Branch preflight counts new rows before private copies and
    cumulative deltas; its exact borrowed serializer includes apertures while retaining
    exact absent bytes. Committed or exact-guard copies never omit aperture data.
18. Purpose-named immutable electrical-analysis shells may omit unused apertures only
    when they cannot become committed deltas, branch candidates, expected snapshots,
    native replacements or library exports. Existing DRC/connectivity/routing geometry
    and absence-report bytes remain exact. Add oversized-unused-data/live-state tests.

## Native, SVG and independent fabrication view

19. Explicit native mask and paste layers draw resolved rectangles; None shapes emit
    nothing. Reuse the original part mirror/rotation/translation once, including bottom
    placement. Existing electrical picking and snapping ignore apertures. Bounds/Fit
    and placement ghosts include supported visible shapes with bounded admission.
20. Add explicit top/bottom mask-opening and paste choices to SVG/PNG without changing
    default layer selection or old default bytes. Explicit geometry shares centers,
    local dimensions and model milli-degree orientation with native/fabrication. Auto-fit
    includes only selected emitted shapes. Stable pad UUID metadata distinguishes roles,
    Legacy provenance, explicit geometry and intentional omission.
21. Static mask selection uses nominal zero expansion for Legacy pads and labels that
    policy; desktop retains its old fixed preview. Static paste selection emits only
    explicit non-null paste. No false exact parity claim for Legacy policies.
22. Coordinates reach floats only at existing render/I/O boundaries. Preserve exact
    cardinal/mirror transforms and odd-Nm dimensions; arbitrary milli-degree poses use
    existing finite computed transforms and nearest-Nm center quantization (<=0.5 Nm
    relative to that computed center). Gerber/SVG angles reflect the stored milli-degree
    angle, not a newly rounded/inferred orientation. No analytic arbitrary-angle exactness.
23. Extend the strict independent fabrication reader with `Paste,Top/Bot` rectangular
    flashes/positive polarity. Mask remains openings/negative polarity. Retain bounded
    file/total/command/primitive caps and common-coordinate interpretation. Compare
    independently parsed dimensions, centers, sides and angles with original fixtures;
    unsupported shapes/state/polarity reject, without changing files or session state.

## Manufacturing, saved jobs and releases

24. Standalone fabrication gains default-false `include_paste`. False preserves old
    no-aperture files/bytes. Some mask always uses exact stored dimensions and ignores
    the Legacy expansion parameter; Some omitted mask emits nothing. Untented via masks
    keep the old request expansion and no via paste is inferred. Non-null stored paste
    with `include_paste:false` rejects explicitly rather than silently dropping it.
25. True emits deterministic `top-paste.gbr` and `bottom-paste.gbr`, including empty
    sides, positive polarity and only explicit paste rectangles. Legacy pads contribute
    no paste; a conditional aperture summary reports unconfigured/explicit/omitted counts,
    actual mask/paste side counts and incomplete aperture coverage for that subset.
    New metadata is omitted when absent and no new layer is requested.
26. Saved output job **5** requires `include_paste:true`; optional Legend and existing
    calibrated/populated companions compose, with populated still requiring calibrated.
    Older job1..4 forbid the new field, including false/null, and refuse any Some aperture
    container before output publication. Their old absent-feature defaults/bytes/fixed
    file matrices remain exact. Job5 requires an explicit Some policy for every SMD pad:
    deliberate null shapes satisfy intent; Legacy does not. This avoids treating an
    unconfigured paste pad as an intentional omission in a saved release. Borrowed
    preflight also rejects any Part with `pads:None`: legacy preview components have
    unknown pad membership and cannot satisfy complete explicit paste intent. Known
    empty membership `pads:Some([])` is allowed. Unassigned through vias are unaffected;
    their existing mask/drill behavior does not imply an SMD paste policy.
27. Job5's exact file matrix adds both paste files, optional T119 Legend pair, old
    companion relationships and optional checked DRC report. Choose version5 when paste
    is requested,4 when only Legend is requested, otherwise preserve1..3 selection.
    All release files come from one bounded owned snapshot/permit/cursor, using existing
    synchronized NEW atomic directory publication and latest-native protection.
28. Extend the independent saved-release reader's strict version/flag/file matrix and
    aperture count/polarity checks. It verifies observed file facts/count consistency,
    not source authentication, manufacturing completeness, stencil suitability or DFM.
    Existing drill/profile/copper bytes and origin convention are unchanged. No NPTH,
    slots, solder-mask web, paste bridging, courtyard/silk or assembly DRC is added.

## Acceptance and verification before merge

- Exact Legacy native/library/scene/default SVG/fab/job/release/DRC bytes; explicit both-null
  vs Legacy; mixed14/13/12/10 max and clearing; native/log base/before/after old-header
  refusal; library1/2 forbidden presence and library3 real-presence requirements.
- Exact positive/negative margin calculations, odd-Nm doubled-corner oracle on all
  cardinal/mirror/bottom poses, invalid collapse/overflow/centered extents and row-poison
  precedence; preserved IDs/nets/full Undo/redo/no-op and dirty/shared session status.
- Actual initialized two-client MCP/CLI guards including peer edits/same-ID reopen,
  shared editor Save NEW/load/apply/undo, lifecycle/collision preservation and compact
  stable-ID effective-clip input; readable native wide/compact self-closing captures.
- Native/SVG/independently parsed mask/paste Gerber agreement for explicit shapes,
  selected-layer isolation and Legacy provenance; job5 complete explicit policies,
  rejection of unknown `pads:None` versus allowed `pads:Some([])` and unaffected
  unassigned through vias; all exact versioned matrices/companions, empty sides, inspector malformed inputs,
  snapshot/row/result bounds and atomic no-staging failure behavior.
- Linux fmt/strict workspace all-target Clippy/workspace test/build, actual generated
  catalog/workflow and retained runtime/framebuffer/source hashes; independent review
  and exact-head CI before parent merge. No own merge or Done action.
