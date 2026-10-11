# Explicit footprint silkscreen and rectangular courtyards

Ticket: T-0119, S-035. Public behavioral contract, written before implementation.
Predecessor: T-0116. Initial spec worktree uses immutable T115 `61e73f220e5f92259ac2d43b48ba096da70b3e89`;
only this ticket's commits will transplant onto literal final T116 before Rust edits.

## Sources and scope

The [KiCad9 PCB manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)
describes footprint graphic layers and closed courtyard extents, with overlap checking
as a separate check. Silkscreen clipping is separately configurable. This increment
stores and draws shapes without promising clipping or courtyard checking.
The [Ucamco Gerber Layer Format2026.05](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf)
sections4.7/4.8 define linear plots; section5.6 defines Legend Top/Bot separately from
mask, paste and copper. This implementation uses that bounded line-layer subset.

All limits and schemas below are independent Viamind choices. No proprietary code,
assets or binary inspection is used. General import fidelity, text, arcs, polygons,
fills, keepouts, drills, mask/paste rules and courtyard/silkscreen DRC are separate work.

## Stored local geometry and compatibility

1. `Part.artwork: Option<Box<FootprintArtwork>>` and the detached Footprint's optional
   artwork carry the same local geometry. Artwork has an ordered silkscreen segment
   list (`start`, `end`, positive `width`) and an optional rectangular courtyard
   (`min`, `max`, positive drawing stroke width). All coordinates/lengths are Nm.
2. Shapes belong to the component's mounted side. Bottom placement moves them to
   bottom silk/courtyard layers and mirrors localX once. No independently chosen
   opposite-side graphics are supported in this increment.
3. `None` omits the serialized field and preserves the legacy synthetic desktop body.
   `Some` replaces that body. `Some(empty)` intentionally suppresses synthetic artwork;
   it is meaningful and serialized. Pads/nets never infer silk or courtyard geometry.
4. Native project feature13 is required when artwork is Some, even empty. Compose
   required versions by maximum with package12, via rules10 and assembly9. Clearing
   artwork can lower native minimum when no other feature requires13. Native/event
   readers reject older headers carrying artwork in base or before/after Part deltas.
   Event writers retain their existing highest-supported-header policy, advanced to13.
5. Detached footprint library2 is emitted when artwork is Some; library1 otherwise
   remains byte-identical. A library1 input containing an artwork member, including
   null, rejects; library2 can carry the explicit empty container. Unknown fields reject.
6. No new document UUIDs are assigned to artwork rows: whole artwork is a guarded Part
   property. Desktop rows use transient stable UUID widget keys. Segment order is
   retained for deterministic persistence and export.

## Validation, admission and borrowed capture

7. At most256 silk segments and one courtyard per footprint. Coordinates are within
   +/-10000mm; positive widths at most10000mm; segment endpoints must differ and
   rectangle min<max on both axes. Checked arithmetic validates stroked local extents
   within +/-10000mm before transform/output allocation. No automatic geometry repair.
8. Borrowed row counts precede row validation/index/copy. Native artwork deserialization
   refuses excess rows before decoding their owned values. Detached files keep the
   existing1MiB cap; new raw/typed command preflight validates bounded artwork and
   expected-Part row/text budgets before secondary owned cloning. The transport's
   initial JSON decode is not described as allocation-free or universally bounded.
9. Extend inherited aggregate budgets with one structural row per segment, courtyard
   and its four emitted edges where appropriate. Render/fabrication budgets count
   actual emitted primitives before allocation; existing output byte caps remain.
   Branch admission counts artwork before private project copy and cumulative deltas;
   its exact borrowed `BorrowedPart` serializer includes the optional artwork in its
   existing serialized byte counter, preserving exact absent-field representation.
10. Audit every Part/Board copy: native validation/save, guarded expected snapshots,
    selected-Part queries, placement snapshots, detached export, branch candidate/deltas,
    scene/view capture and fabrication snapshots. New artwork must never evade a
    consumer's pre-copy structural budget. No global native-project file cap is added.
11. Electrical-only DRC/router/cleanup/continuity captures may explicitly omit artwork
    before copying unused data. This is a private immutable analysis shell only.
    Omission must never reach a live committed Part delta, exact expected guard,
    persistent branch candidate, document replacement, native save or exported footprint.
    Add a huge-unused-art regression and exact live-state preservation checks.

## Shared engine, library and desktop workflow

12. `set_footprint_artwork` takes exact project/Part/fullcursor guards and requested
    optional artwork. Any stale target/stream/sequence rejects atomically. Same initial
    transaction cursor applies to a batch. One changed Part delta/undo; no-op no event.
    Pads/nets/IDs/pose/schematic links/assembly metadata remain exactly unchanged.
13. Detached editor gains stable-ID silk rows and optional courtyard exact-decimal fields,
    local preview and explicit Legacy body preview / Explicit artwork choices. Save NEW,
    load and export preserve artwork through the same native file lifecycle guard.
14. Apply library2 atomically replaces pads/name/artwork. Matching pad numbers retain
    padIDs/nets; placement/link metadata remain unchanged. Applying library1 explicitly
    restores legacy artwork mode. Extend apply/import with optional fullcursor, required
    when either source or target has explicit artwork; legacy library1-to-legacy callers
    retain the old omitted-cursor behavior. Pad-only commands preserve artwork.
15. Desktop application uses shared engine/MCP commands. Preparing/editing/Cancel is
    local only; Apply recomputes validation under exact guards. Peer edits or same-ID
    project reopen reject stale apply. Existing conflicting editor guards remain intact.
16. At640x400, persistent footer actions precede the bounded scrollable row area. Stable
    widget IDs survive deletion of preceding rows. Actual input tests cover author,
    remove/edit, Save NEW/load/Apply/undo, Cancel and stale/reopen behavior.

## Shared placement, native and view geometry

17. Reuse actual shared mirror/CCW-y-down-rotation/translation for all endpoints and all
    four courtyard corners. Quarter-turn transformations are exact integer Nm.
    Arbitrary poses remain supported at the existing f64 render/I/O boundary.
18. SVG/Gerber quantize transformed coordinates to nearest Nm after finite/range checks.
    The <=0.5Nm bound is relative to the finite computed transform, not a claim of exact
    arbitrary-angle analytic coordinates. No floating geometry is stored in the model.
    Width is retained; bottom manufacturing files use common top view without remirroring.
19. Desktop silk/courtyard layers, scene/Fit/selection bounds and placement ghosts use
    stored geometry. Courtyards are boundary strokes only. Legacy None scenes retain
    their existing synthetic silk body. Electrical pad/track/via picking remains valid.
20. SVG/PNG adds explicit top/bottom silkscreen/courtyard choices. Preserve existing
    default layer selection and old no-art bytes. New layers emit stored artwork only,
    never synthetic bodies. Metadata accurately states absent/excluded geometry and
    unperformed checks. Shared transformed points/widths drive parity fixtures.

## Explicit manufacturing and checked releases

21. Fabrication gains default-false `include_silkscreen`. False preserves the existing
    old-file set/bytes when metadata is absent and explicitly discloses excluded artwork
    when present. True emits deterministic top/bottom legend files, including empty
    sides, positive polarity, circular apertures and D02/D01 straight strokes with
    `Legend,Top`/`Legend,Bot` attributes. No automatic silk clipping.
22. Courtyards are not emitted as copper/profile/production legend. DRC still reports
    silk/courtyard checks unperformed; artwork creates no electrical net/anchor/obstacle.
23. New checked output job4 requires `include_silkscreen:true`. Existing v1-v3 job bytes,
    defaults, validation and fixed file matrices remain exact. Job4 permits existing
    calibrated/populated companions only under their existing relationships; populated
    still requires calibrated. Legend flag on an older job rejects rather than disappears.
24. Job/release/fabrication manifests record the new layer files/counts and captured
    project/cursor. The independent release reader accepts exactly the versioned file
    matrix and retains all old matrix checks. No implicit approval or readiness claim.
25. Extend the independent fabrication-view reader with Legend Top/Bot line layers and
    overlay display, retaining existing file/aggregate command/primitive caps. Compare
    independently parsed endpoints, widths, side and common-origin facts with source
    geometry; malformed/unsupported files refuse with existing atomic publication rules.

## Acceptance and required verification

- Exact legacy native/library/report/job/scene byte/geometry parity when artwork absent;
  explicit empty suppression; mixed13/12/10/9 max composition and clearing; old native
  and event base/before/after refusal; malformed/oversized raw-before-poison cases.
- Odd-Nm quarter/mirror independent expected geometry, arbitrary finite-boundary
  quantization parity and stroked overflow; shared native/SVG/parsed-Gerber parity.
- Typed/dynamic/batch fullcursor/object/reopen conflicts unchanged; no-op and oneundo
  retain electrical IDs/nets/links/metadata; actual initialized two-client MCP/CLI proof.
- Actual compact stable-ID author/edit/remove/save/apply/cancel tests, plus readable wide
  and compact native captures with exact original RGBA/RGB provenance and invariants.
- Borrowed capture/delta/branch/serialized limits, huge-unused-art analysis omission,
  old and new exact release matrices and failure/no-staging publication regressions.
- Linux fmt, strict workspace all-target Clippy, workspace tests/build; actual generated
  tool reference/workflow checks; independent review and exact-head CI before merge.
