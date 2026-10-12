# Bounded circular DXF outline conversion

T-0128, S-050. Public-source behavioral contract written before Rust changes.
Dependencies T-0039 and T-0043 are merged. Independent approval is required before
implementation. This extends the importer described in
[the straight-edge contract](dxf-outline-import.md).

## Sources and supported claim

Only published format and platform documentation is used:

- Autodesk [ARC group codes](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-0B14D8F1-0EBA-44BF-9108-57D8CE614BC8.htm),
  [CIRCLE group codes](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-8663262B-222C-414D-B133-4A8506A27C18.htm),
  [LWPOLYLINE group codes](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm),
  [VERTEX group codes](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-0741E831-599E-4CBF-91E1-8ADBCFD6556D.htm),
  and [2014 DXF reference](https://images.autodesk.com/adsk/files/autocad_2014_pdf_dxf_reference_enu.pdf).
- Autodesk [bulge definition](https://help.autodesk.com/cloudhelp/2023/DEU/AutoCAD-ActiveX-Reference/files/GUID-E1CE125E-AB3A-4645-B548-E43200064F9C.htm),
  [OCS conventions](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-D99F1509-E4E4-47A3-8691-92EA07DC88F5.htm),
  [arc direction](https://help.autodesk.com/cloudhelp/2026/ENU/OARX-ManagedRefGuide/files/OARX-ManagedRefGuide-Autodesk_AutoCAD_DatabaseServices_Arc_StartAngle.html),
  [DXF angular units](https://help.autodesk.com/cloudhelp/2018/ENU/OARX-RefGuide/files/OREF-DXF_Group_Codes.html),
  and [drawing units](https://help.autodesk.com/cloudhelp/2020/ENU/AutoCAD-DXF/files/GUID-A85E8E67-27CD-4C59-BE61-4DC9FADBE74A.htm).
- Rust [floating-point operations and their precision contracts](https://doc.rust-lang.org/std/primitive.f64.html),
  [File descriptor metadata](https://doc.rust-lang.org/std/fs/struct.File.html#method.metadata),
  pinned workspace [rustix 1.1.5 file flags](https://docs.rs/rustix/1.1.5/rustix/fs/struct.OFlags.html),
  [sha2 0.10.9](https://docs.rs/sha2/0.10.9/sha2/), and
  [egui 0.36.2 ScrollArea](https://docs.rs/egui/0.36.2/egui/containers/scroll_area/struct.ScrollArea.html).

DXF stores circle center/radius and arc start/end angles; bulge is the tangent of
one quarter of a segment's included angle, with its sign determining direction.
These statements define source interpretation. The numerical limits, explicit
conversion report, transactional guards and desktop workflow below are Viamind
choices. Mathematical bounds below are independently derived from circle geometry.

The result is a validated **polygonal approximation** of supported source geometry.
Native BoardOutline, native schema and event-log versions remain unchanged. DRC,
manufacturing and later editing operate on that polygon. Analytic native arcs,
editable native rounded slots, exact curved Gerber output, certified fabrication
equivalence, splines and general DXF fidelity remain outside this ticket. The S-050
analytic-curve story acceptance stays open; a rounded-slot *polygon conversion*
fixture does not satisfy it.

## 1. Compatibility and explicit entry points

1. Preserve the existing text command `import_board_outline_dxf`, its straight-only
   refusal behavior, schema/defaults and accepted straight fixture coordinates and
   ordering. It does not silently start converting curves. The existing manual
   outline form and one BoardOutline delta behavior remain available.
2. Add shared `preview_board_outline_dxf` and `apply_board_outline_dxf` operations.
   Input chooses exactly one bounded UTF-8 text or local path source, existing
   `header`/`mm`/`inches` units, optional exact layer, and required integer
   `tolerance_nm`. There is no implicit curve-conversion tolerance. Desktop displays
   its initially proposed value (10000 Nm) and sends that value explicitly.
3. New operations accept the straight subset and the circular subset below.
   Straight-only conversions preserve predecessor polygon bytes; the report still
   discloses inherited source-to-Nm rounding. Tolerance governs every converted
   curved segment's total boundary deviation, not an automatic endpoint join radius.

## 2. Selected source geometry

4. Accept a single ASCII/UTF-8 DXF ENTITIES section with existing section/EOF rules.
   Retain explicit units, exact layer matching and the current POLYLINE parent-layer
   selection rule. All child VERTEX records belong to that selected parent even
   when their redundant layer fields differ. Report that selection convention.
5. Support closed 2D LWPOLYLINE and simple closed POLYLINE/VERTEX/SEQEND with a
   per-vertex bulge on the segment to the next vertex, including the closing segment.
   An absent bulge is zero; a duplicate bulge for one vertex fails. A final explicit
   duplicate of the first vertex can only be removed when its outgoing bulge is zero;
   ambiguous repeated closing geometry fails. Parent dummy origin must remain zero.
6. Support standalone CIRCLE rings and ARC plus LINE chains assembled into closed
   rings. ARC angles are degrees, measured counterclockwise in standard +Z OCS.
   Accept exact bounded angles in [-360,360], normalize modulo 360, and use the
   positive sweep from start to end. Equal normalized angles are ambiguous/degenerate
   and fail; a full circle must use CIRCLE. Do not infer a clockwise standalone ARC.
7. Support bulges of either sign with `0 < abs(bulge) <= 1` (at most a semicircle).
   A zero bulge is a line. Larger bulges are explicitly unsupported in this increment;
   the error suggests splitting the source arc. No tiny nonzero bulge is silently
   discarded. A numerically ill-conditioned tiny bulge can be rejected with its
   entity/vertex and a split-or-straighten-in-source instruction.
8. All selected geometry must have zero elevation, thickness and width, standard
   extrusion (0,0,1), model space and supported 2D flags. Reject negative radii,
   zero radii/chords, mesh/spline flags, nonstandard OCS, INSERT/BLOCK expansion,
   ELLIPSE/SPLINE/HATCH and other selected entity types. Ignore only explicitly
   unselected entity geometry and non-ENTITIES sections as in the existing importer;
   still charge them against scan/pair limits. Report ignored entity counts.
9. DXF X is native X; DXF Y is negated once. No origin shift, scaling inference,
   axis rotation, snapping, welding or shape repair is offered. Unit overrides are
   explicit and disclosed. Endpoints join only when their independently converted
   integer Nm coordinates are exactly equal. The report discloses each endpoint's
   quantization bound: equality after the normal Nm conversion is not a claim that
   distinct source coordinates were exactly equal before conversion.

## 3. Analytic error and deterministic subdivision

10. Accept `tolerance_nm` in [10,1000000] inclusive. Smaller values are outside this
    implementation's numeric/quantization contract; larger values would make a
    misleadingly coarse board and require an explicit later contract. Radius must
    be in [1,10000000000] Nm after explicit unit conversion. Curve centers and full
    curve extrema must be inside the existing +/-10000000000 Nm native domain;
    a curve leaving it fails even if the chosen sample vertices would fit.
11. Source numeric tokens have bounded decimal grammar: optional sign, decimal
    digits/point, optional decimal exponent; no NaN/Infinity/hex. At most 32
    significant digits, 64 token bytes and exponent [-32,32]. Treat those decimal
    values as the source semantics, not as already rounded f64 inputs. Exact rational
    or outward-enclosed conversion must account for parse and unit-conversion error.
    Straight-only legacy entry behavior is unchanged by these new-entry limits.
12. Each ideal circular subarc and its chord have maximum sagitta
    `r * (1 - cos(abs(sweep)/2))` for subarc sweep at most pi. For equal n-way
    subdivision replace sweep by sweep/n. A corresponding bulge chord has sagitta
    `abs(bulge) * chord_length / 2`. Reserve the additional source/interval evaluation
    and emitted vertex-rounding displacement before accepting a segment. Vertex
    nearest-Nm rounding alone has Euclidean bound sqrt(1/2) Nm; report a conservative
    integer upper bound, never zero for an uncertified rounded point.
13. Use a fixed deterministic subdivision order (source entity order, source segment
    order, then increasing parameter) with bounded bisection depth and checked work.
    Split arcs at quadrant boundaries before refining; CIRCLE begins at +X and
    proceeds counterclockwise in DXF coordinates. Preserve explicit segment endpoints
    and reuse the identical emitted endpoint at a join. Do not drop repeated rounded
    vertices to rescue a tolerance or vertex budget: collapse fails actionably.
14. Every emitted chord must have a **conservative analytic upper bound** on the
    deviation between the source subarc and that rounded chord. Combining sagitta
    and endpoint displacement uses the triangle inequality, so it bounds the entire
    subarc and chord, not merely sampled points. Round bounds outward/upward to Nm.
    Acceptance requires each bound <= requested tolerance. The achieved report is
    the maximum of those accepted upper bounds; it is not the measured exact error.
15. Ordinary platform `sin`, `cos`, `atan`, `hypot` outputs and a guessed epsilon
    are insufficient certificates: Rust documents unspecified transcendental
    precision. Bound any needed transcendental evaluations with a documented finite
    range-reduced series and remainder/roundoff enclosure, or derive subdivisions
    algebraically with bounded square-root intervals. Bounds on numeric evaluation
    and source conversion must be propagated through conditioning, not assigned a
    universal unproved epsilon. Refuse unresolved rounding/domain/conditioning or
    exhausted work/depth with an actionable error. Tests include rational/quadrant
    exact cases plus independent analytic oracles; dense sampling is supplementary.
16. The emitted polygon is validated by existing integer BoardOutline checks:
    degree-two joined chains, nonzero area, no zero edges, overlap, self-intersection,
    touching rings, disjoint boards or nested cutouts. Largest absolute-area ring is
    outer, all others are strict cutouts. Conversion tolerance does not guarantee
    preservation of arbitrary source topology or minimum manufacturing clearances;
    the report explicitly says so. A failed polygon is never partially applied.

## 4. Source, geometry and result admission matrix

All counts use checked arithmetic and borrowed inspection before copying strings,
reserving entity/point/report storage or calling nested typed decoding. Limits are
for the new entry points; inherited straight API limits remain unchanged.

| Stage / resource | Maximum | Admission before |
| --- | ---: | --- |
| UTF-8 source bytes, inline or file | 8 MiB | secondary owned text / pair allocation |
| Local encoded path | 4096 bytes | absolute/canonical path or OS open |
| Layer option / entity layer name | 512 bytes each | copying or matching owned names |
| Physical group-code/value pairs, including comments | 131072 | appending next Pair |
| Individual line | 4096 bytes | retaining/decoding the line |
| Numeric token / significant digits / exponent | 64 / 32 / +/-32 | rational/interval construction |
| All entity records, including unselected and VERTEX/SEQEND | 16384 | retaining entity state |
| Selected source segments/standalone circles | 2048 total | segment allocation/subdivision |
| Polygon vertices across all rings | 2048 total | emitting each point |
| Rings / cutouts | 65 / 64 | retaining another ring |
| Subdivision depth per source segment | 24 | pushing further refinement |
| Numerical refinement/evaluation steps | 262144 total | performing next bounded step |
| Polygon topology edge-pair checks | 4194304 total | checking next pair |
| Fidelity source-segment rows | 2048 | pushing next row |
| Bounded diagnostic | 512 UTF-8 bytes | formatting untrusted excerpts |
| Serialized preview/report plus outline | 2 MiB | response publication |
| Retained importer workers / pending requests | 1 / 0 per form | spawning another worker |

17. Refusal identifies the limit, entity ordinal/type and segment/vertex where
    applicable, and suggests a source-layer filter, split/simplification or larger
    explicitly chosen tolerance. It never quietly raises tolerance, drops an entity
    or truncates geometry/report rows. Report size can be checked by a bounded
    counting serializer before constructing a large owned output value.
18. Borrowed raw-input preflight checks text/path/layer/count/expected-outline sizes
    before typed secondary allocations. Raw typed JSON rejects duplicate fields;
    an already constructed transport Value cannot recover duplicates its transport
    parser discarded, which is documented honestly. Missing/invalid fields and
    oversized max+1 poison rows have deterministic negative precedence tests.

## 5. Fidelity report

19. A deterministic preview contains SHA-256 of the actual source bytes, explicit
    units/override, layer selection, requested tolerance, achieved analytic upper
    bound, source/selected/ignored counts, output vertex/cutout counts, the polygon,
    bounds, and one bounded row per selected source segment/circle. Rows identify
    source entity ordinal/type, optional bounded handle (report-only; not persisted),
    vertex/segment ordinal, line/arc/circle/bulge classification, emitted chord count,
    source/numeric enclosure and quantization bounds, achieved total bound and
    conversion disposition. No per-edge native UUID preservation is claimed.
20. Report notices state polygon-only native geometry, inherited endpoint rounding,
    Y convention, selected/ignored scope, no welding, unsupported types, and no
    source-topology/fabrication/clearance guarantee. For straight-only geometry the
    report distinguishes source rounding from zero curvature sagitta. Repeated
    preview of identical bytes/options has deterministic polygon/report content;
    session source cursor is separately identified runtime metadata.

## 6. Safe read, guards, history and admission

21. Path reads occur on a retained worker outside the design mutex. On Linux open
    read-only with NONBLOCK|CLOEXEC through existing safe rustix, then inspect metadata
    on that same opened descriptor and require a regular file. A preliminary path
    metadata check never substitutes for the descriptor check. Read at most limit+1
    bytes and reject oversized/non-UTF-8 input. Handle a raced FIFO/device/directory
    without blocking. No source writes, temporary outputs or external DXF resources.
    A symlink may select a regular file; digest binds actual read bytes, not a claim
    that the path remained attached to the same inode afterward.
22. Preview is an engine observation under the existing read guards/mode admission;
    it cannot mutate history/dirty state. Apply is explicitly classified as a writer
    and refused centrally in restricted agent modes before filesystem/worker work.
    Its path I/O and conversion finish before acquiring the design lock.
23. Apply requires source/options, expected source SHA-256, project UUID, full cursor
    (including stream identity), and exact current ExpectedOutline. It rereads and
    reparses the source, compares digest, and recomputes the same bounded plan; no
    opaque preview token or trusted client polygon bypass. Under the existing atomic
    command transaction, compare all project/cursor/outline guards again and apply
    one BoardOutline delta. A change/reopen/new stream or unrelated design edit
    invalidates Apply. Equal geometry is a no-op after successful guards. Undo/redo,
    native save/reopen and log replay use the existing polygon delta. Other board
    objects, unsaved edits, active native path and source file bytes are preserved.

## 7. Desktop lifecycle and actual evidence

24. Extend only the existing outline importer UI/controller. Keep manual outline
    editing available. Curve import has distinct Preview and Apply controls, explicit
    tolerance, a readable approximation warning and report rows. Success reports
    the imported polygon counts/error; Apply is disabled until the current form's
    preview and full engine context match. Path/units/layer/tolerance changes clear
    preview freshness. A source-file change is caught by Apply digest/reparse.
25. Keep one retained worker receiver through Close/reopen; weak engine ownership
    and fallible spawn preserve app lifetime. Project replacement, local draft
    generation and source cursor reject stale completions. Busy controls cannot
    reset or overwrite a newer draft; no detached task mutates a design. Escape/Close
    preserve source and local form unless explicit reset is chosen. Retaining a
    closed form's pending receiver does not auto-Apply its eventual preview.
26. Bound the modal to the available viewport and use a finite scroll area with
    reachable Preview/Apply/Cancel/error/report at 640x400. Actual input tests reveal
    each target, assert its whole response inside all ancestor clips, and click/type
    only then. New tests cover busy/close/source-change/reopen/stale/failed-spawn and
    both text and file MCP parity. Existing unrelated editor/worker layouts stay
    outside scope; no broad loader or desktop refactor.

## 8. Required verification and remaining story gap

27. Independent geometric fixtures: exact semicircle bulges +/-1, rational bulge
    both directions, quarter ARC, angle wrap, asymmetric circle/cutout Y orientation,
    odd Nm center/chord/radius, and LINE+two-semicircle rounded-slot ring. Use an
    independent analytic sagitta/radial/corner oracle, verify signed direction and
    full interval certificate, not only area/rectangle/sampled agreement. Reverse
    winding and source reorder tests preserve validated geometry classification.
28. Negative fixtures cover every source budget at boundary/max+1, poisoned excess
    records, nonfinite/duplicate/oversized numbers, |bulge|>1, tiny ill-conditioned
    bulge, too-tight tolerance, collapsed vertices, nearly touching/crossing rings,
    ambiguous/open endpoints, wrong extrusion/elevation/width, unsupported selected
    entity, curve extrema outside native range and subdivision exhaustion. A
    controlled regular-file-to-FIFO swap proves same-fd nonblocking refusal.
29. Engine and real two-client MCP checks prove preview immutability, exact digest
    and complete cursor/project/outline guards, peer stale rejection, per-client
    undo ownership, one delta/no-op, undo/redo/native/replay, and restricted-mode
    early writer refusal. Retain all failure receipts without claiming them PASS.
30. Linux fmt, strict workspace all-target Clippy, full workspace tests/build, actual
    tool catalog/workflow parity and exact-head CI must pass. Retain unique binaries,
    original source refs and reproducible production checks. Use readable normal and
    compact actual WGPU captures with full-session assertions/self-close and original
    pixel provenance; describe inspection event injection accurately. Independent
    review follows concrete evidence; a human owns final Done.

S-050 visual: straight source and supported circular source -> bounded certified
conversion report -> guarded one-delta polygon -> existing render/DRC/fab. Native
analytic curves and editable native rounded slots remain a separate open path.
