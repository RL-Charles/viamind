# Optional calibrated placement in saved manufacturing jobs

Ticket: T-0104 · Story: S-077 · Status: proposed for independent review before code.

## Public sources

Reviewed 2026-10-11; behavioral sources only, no proprietary file compatibility:

- [KiCad9 CLI jobset commands](https://docs.kicad.org/9.0/en/cli/cli.html#_jobset_commands): saved jobs run against an explicit project, selectable destinations and failure handling. This motivates reusable explicitly chosen outputs; Viamind keeps its own bounded detached JSON and atomic release contract.
- [KiCad9 position export](https://docs.kicad.org/9.0/en/cli/cli.html#_pcb_position_file_export): explicit units, sides, origin/bottom-coordinate and DNP choices. These are choices that must be declared, rather than assumed universal conventions.
- [JLCPCB placement requirements](https://jlcpcb.com/help/article/pick-place-file-for-pcb-assembly): reference, centroid in millimetres, side and counterclockwise rotation. This does not establish a universal package zero or bottom marker calibration.
- [PCBWay assembly file requirements](https://www.pcbway.com/assembly-file-requirements.html): position/orientation/side and surface-mount centroid information. The generic Viamind companion explicitly includes all entered mounting technologies, so it is not claimed to be that manufacturer's submission format.
- [RFC8259 section9](https://www.rfc-editor.org/rfc/rfc8259#section-9): JSON implementations may bound input size/depth/strings. Reuse the existing64KiB detached configuration reader.
- [Rust thread Builder::spawn](https://doc.rust-lang.org/std/thread/struct.Builder.html#method.spawn) and [rustix no-replace rename](https://docs.rs/rustix/latest/rustix/fs/struct.RenameFlags.html#associatedconstant.NOREPLACE): retain recoverable named worker creation and atomic NEW release visibility.

Existing independently reviewed [saved jobs](saved-output-jobs.md), [checked jobs](checked-output-jobs.md) and [explicit calibration](assembly-placement-calibration.md) define unchanged supported geometry, DRC coverage, reference/population rules and coordinate bytes. No new renderer or manufacturer rotation convention is introduced.

## Detached version2 opt-in

Existing version1 descriptors remain accepted and canonical-byte identical. They cannot request the companion. New descriptors explicitly use both version2 and `include_calibrated_placement: true`:

```json
{
  "format": "viamind-output-job",
  "version": 2,
  "name": "Explicit calibrated release",
  "profile": "generic_outer_2layer",
  "mask_expansion_mm": 0.0,
  "include_calibrated_placement": true
}
```

The same opt-in works with `checked_outer_2layer` and its existing required checks. Profile/geometry/DRC contracts are unchanged. Reject version1 with the new field; version2 without it; false/null/wrong-type values; unknown/duplicate fields; unsupported versions/profiles; incomplete/forbidden checks; invalid existing names/numbers; and configurations above64KiB. Optional-field absence must be distinguishable from an explicit null, and canonical version1 serialization omits the new field entirely. Keep the existing field order and append the new field for version2.

Retain the existing1–256UTF-8-byte nonblank/control-free name, finite mask0–10mm, finite checks0–100mm, max_violations1–10000,4096-byte path and bounded regular non-symlink/NONBLOCK file-reader requirements. No arbitrary output list, path template, script, archive, remote destination or supplied validation report is accepted. This is detached job schema versioning; no native project/event schema change is needed.

## One captured board and one admission slot

Load and validate the detached descriptor outside the engine lock. Under the existing short engine lock, check the complete optional expected cursor first, then acquire exactly one nonblocking shared manufacturing permit. The desktop always supplies its original full cursor. Preserve the captured cursor/native path and immutable descriptor.

Integrate the opt-in purpose into the existing FabricationSnapshot capture before its board clone. Keep all current borrowed budgets:100000 job components,200000 fabrication primitives including headers/pads/outline/policy records,4096bytes per captured board string and2MiB aggregate board text. Validate/charge bounded document net policy before copying its rows/names. Unrelated schematic sheets and source assembly strings must never be cloned or validated for this job. Reference-key allocation happens only after these borrowed budgets and while the single permit is held.

For version2 only, run the existing calibrated-placement scalar/completeness/reference validation against the borrowed board. Population defaults true when metadata is absent. Every populated component needs explicit mounting and local centroid/marker; missing/invalid data or case-colliding/invalid references refuses the complete release. Do-not-populate components are excluded from the companion and do not require calibration. All explicit mounting technologies remain included; no pad/package/vendor inference is permitted.

After validation, clone the bounded geometry once using the existing assembly-free geometry path and retain only the fixed scalars needed for the companion: population, mounting and local centroid/marker. For DNP components only the false population flag is required. Do not copy manufacturer, MPN, supplier or SKU strings. The default version1 capture continues to strip all assembly metadata. Borrowed count/text/scalar validation must precede every new string/map/board copy. Do not obtain an AssemblySnapshot or second manufacturing permit.

Every output and the trusted production DRC callback use this same owned board/rules. Later model edits, undo, descriptor-file changes or caller cancellation cannot change the captured release. Checked jobs continue to enforce central requested and captured document minima, complete supported geometry coverage and violation refusal; no caller-supplied pass/report is admitted. Keep the permit owned by the blocking/retained worker until completion/drop, including after an async caller is canceled. At most two manufacturing snapshots exist per process.

## Output files, limits and honest manifest

Version1 files and every accepted byte remain identical. Version2 adds exactly one fixed file:

- `assembly/calibrated-placement.csv`: byte-identical to standalone `assembly_placement` from the captured source, with deterministic reference/UUID ordering, existing escaping and populated-only included count.

The existing `assembly/bom.csv` and `assembly/placement.csv` remain unchanged inventories of all components, with component-origin coordinates. All supported fabrication files, its own fabrication manifest, and any checked DRC report remain unchanged. Generic version2 produces12 files and checked version2 produces13, including the release manifest. Descriptor `job.json` is the exact canonical validated version2 configuration actually used.

Use release-manifest version2 only when the companion is requested. Append an optional `calibrated_placement` summary containing its fixed relative `file`, actual `component_count` (0–100000) and static `coordinate_convention: "global_x_right_y_up_ccw_common_top_view"`. Version1 remains version1 and omits this summary so its canonical bytes stay identical.

Version2 exclusions must explicitly distinguish the old all-component generic inventories from the populated calibrated companion; do not retain the false blanket statement that no population metadata is used. Explain that generic BOM/origin placement are not a matched populated assembly set, no sourcing data is included, and the calibrated generic marker does not establish a universal vendor zero, polarity/tape convention or manufacturer acceptance. Fabrication exclusions and its own manifest bytes stay unchanged.

The existing transform remains: local X reflection on bottom, part rotation then translation; output globalX and negative worldY; top marker r+theta, bottom r+180-theta modulo360degrees. Preserve all existing scalar/reference bounds and exact-quarter-turn versus finite rounded non-cardinal behavior. Output units, headers and decimal precision are the standalone format's exact bytes.

Keep32MiB per CSV,128MiB fabrication,16MiB checked report and192MiB complete release limits. Precharge the remaining release budget before rendering/appending the companion, descriptors and manifest. No unbounded path list or filename exists. Stage/sync all fixed files and bounded subdirectories in one sibling directory, then publish the complete NEW root with Linux NOREPLACE. Existing files/directories/symlinks/native destinations refuse; no overwrite or partial success. Failures clean ordinary staging and leave no final root. Process interruption may leave a named stage; atomic visibility is promised, not power-loss durability.

## Shared access and desktop

Existing load/save/run MCP tools and CLI `--run-output-job` consume the same typed descriptor/capture/writer. Regenerate actual canonical MCP schemas/descriptions. Read-only agent admission continues to permit only existing trusted observations and exclude save/run; no new mode bypass.

The saved-job desktop adds one explicit optional calibrated-placement checkbox, off by default. Saving selected settings emits version2/true; clearing emits unchanged version1. Loading sets this state. Run reads the actual saved file, and the existing changed-form notice remains visible so an unsaved checkbox edit is not mistaken for a changed saved job. Keep one retained fallible named worker, disabled pending controls, hidden polling, vertical scroll, source/full result cursor and newer-edit message. Complete result reports the optional companion count without changing existing result fields. No engine model mutation or new UI-only export path is introduced.

## Required verification

Before submit: all four Linux workspace gates, final exact-head CI, current canonical reference generation/drift and independent review. Meaningful regressions include:

1. Strict version1/version2 acceptance/rejection and old canonical bytes, including null/false/duplicate/unknown/deep/overbudget settings and FIFO/symlink protections.
2. Same-snapshot standalone companion byte parity with asymmetric top/bottom, non-cardinal transforms, Unicode/quotes/newlines, mounting and DNP counts; old generic/fabrication bytes unchanged in both profiles.
3. Missing/partial/invalid populated calibration and reference collision atomic refusal; headers-only zero population; unused3MiB/invalid sourcing and unrelated invalid schematic excluded before copying; no source mutation.
4. One retained permit for the complete job, bounded pre-append output and cancellation ownership; stable captured metadata/rules after later source edits; stale cursor before admission; checked requested baselines cannot weaken captured document policy.
5. Atomic NEW collision/raced publication/injected failure/native protection with no partial release. Retain existing lifecycle and stage boundaries.
6. Actual compact desktop checkbox/load/save/run/pending/result/error input, CLI and two initialized MCP clients producing equal bytes and whole-session invariants. Durable exact-source production plus self-closing native-framebuffer evidence clearly separates any inspection wrapper and synthetic local input.
