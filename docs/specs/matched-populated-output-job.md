# Matched populated generic BOM and placement companions

Ticket T-0108, S-077. Public-source clean-room behavior, written before implementation. This increment depends on reviewed T-0104 version2 calibrated jobs; this spec-only branch starts from merged main3c52571 and will integrate T104 before Rust changes.

## Public sources and limits of inference

Read on2026-10-11:

1. [KiCad9 CLI position export](https://docs.kicad.org/9.0/en/cli/cli.html#_pcb_position_file_export) exposes explicit units, side, bottom-X/origin and DNP/SMT filters. Its schematic BOM export separately exposes DNP exclusion and fields/grouping. This supports explicit selection of the same population for BOM and placement; it does not prescribe a universal placement convention.
2. [KiCad9 jobset commands](https://docs.kicad.org/9.0/en/cli/cli.html#_jobset_commands) run saved settings against an explicit project/destination and provide stop-on-error. Viamind retains its stricter coherent-capture, all-or-nothing NEW release behavior.
3. [PCBWay assembly file requirements](https://www.pcbway.com/assembly-file-requirements.html) distinguish BOM quantity/reference/part/package/mounting and optional manufacturer/distributor information from SMT-only centroid position/orientation. This generic increment includes every explicitly populated mounting type in both companion files. It does not implement a PCBWay centroid template or acceptance claim.
4. [JLCPCB BOM requirements](https://jlcpcb.com/help/article/bill-of-materials-for-pcb-assembly) describe matching BOM parts to layout references, package/value information and case-insensitive reference handling. This supports retaining explicit stable component identity and using the existing calibrated reference collision checks. No vendor identifier or approval is inferred.
5. [RFC4180](https://www.rfc-editor.org/rfc/rfc4180) describes consistent CSV fields and quoting commas, quotes and line breaks. [RFC8259 section9](https://www.rfc-editor.org/rfc/rfc8259#section-9) permits bounded JSON parsing. These are format/bounds references, not fabrication certification.

Viamind's scalar transform, source cursor, stable component IDs, job descriptor versions, fixed filenames, limits and provenance digest are local design choices. Existing T87/T100/T104 sourcing/calibration/one-snapshot behavior remains the implementation authority. No proprietary source/assets or manufacturer uploads are needed.

## Problem and scope

Version1 jobs deliberately preserve all-component generic BOM/origin inventories. Version2 adds one explicitly calibrated populated-only placement companion and discloses that those older inventories have a different population. Add an optional generic populated BOM inventory with exactly the same component IDs as that placement companion. Preserve all v1/v2 canonical settings, manifests, file lists and format bytes by default.

No model/native/event schema change, standalone CSV catalog expansion, vendor template, stock/procurement lookup, inferred MPN/package/mounting/centroid/polarity/vendor zero, grouping across differently identified components, or complete assembly acceptance. Existing generic/vendor standalone CSV formats are unchanged. This is a fixed saved-job output composition increment through shared CLI/MCP/desktop run/save/load paths.

## Strict explicit saved settings

Add `include_populated_bom: Option<bool>` with absence distinct from present null. Accept only:

| Version | Calibrated placement | Populated BOM |
| --- | --- | --- |
| 1 | absent | absent |
| 2 | present true | absent |
| 3 | present true | present true |

Present false/null, wrong types, missing required true, new flag on versions1/2, duplicate/unknown fields, unsupported versions or free-form output lists reject atomically. Preserve existing generic/checked profile names, validation checks and canonical serialization field order/omission for old descriptors. Manual JSON schemas must express the same combinations. Config remains at most64KiB, bounded depth, strict regular non-symlink/nonblocking opened-file checks and NEW-only save.

## Exactly matched generic population

Use the existing `populate` contract: absent/default metadata means populated; explicit false excludes the component from both companions. Every included component must already pass T104 explicit mounting/local centroid/marker/reference scalar preflight. All explicit mounting types participate; no SMT-only filtering. Empty population is allowed and produces headers-only companions with count0.

Fixed new output: `assembly/populated-bom.csv`. UTF-8 without BOM, CRLF records and existing CSV escaping. One row per included component, quantity1, deterministic stable component-ID order. Exact headers:

```text
Designator,Quantity,Value,Footprint,Mounting,Manufacturer,MPN,Supplier,Supplier SKU,Component ID
```

Mounting uses existing explicit `surface_mount`, `through_hole`, `hybrid`. Value/footprint/reference/ID remain exact model values. Four optional sourcing strings are exact user-entered values; absent identifiers render empty cells. Missing identifiers do not imply a valid procurement choice. Present invalid strings reject the complete release. Do not replace missing MPN with value/footprint or infer any sourcing from imported property names.

The existing calibrated companion bytes remain exact standalone calibrated renderer bytes. The old generic BOM/origin files remain all-component inventories with unchanged bytes. Tests must independently compare component-ID/reference sets across both companions, including mixed mounting, top/bottom, DNP, duplicate excluded references and missing explicit calibration.

## One coherent bounded capture and permit

Acquire the existing one manufacturing admission permit, check full cursor before admission, then validate borrowed board/rule/primitive/header budgets before cloning. Preserve all current200000 primitive/100000 total component and geometry header/per-field limits. For version3 only, validate included sourcing fields with the existing1–512 UTF-8 byte/no-controls/no-edge-whitespace rules; at most2MiB aggregate included sourcing text. This is an additional purpose-specific cap alongside the unchanged existing2MiB board text cap (at most4MiB combined captured board+source text). Fail before copying sourcing strings or allocating output/reference keys beyond existing bounded preflight. Do not scan/validate/clone DNP unused sourcing or unrelated schematics.

One geometry/rules copy serves generic outputs, both companions and trusted central checked-job DRC. Overlay included fixed calibration/mounting/population and only four required included sourcing strings. DNP retains its false population marker without calibration/sourcing. Default v1 captures no assembly metadata; v2 retains only fixed scalars and still ignores unused sourcing, including huge/invalid direct-project metadata. No second project/AssemblySnapshot or second permit. Existing nonblocking two-slot admission and cancellation lifetime apply through complete staging/publication; no heavy work or awaited lock in the engine critical section. Named-net document minima are retained and central DRC remains authoritative; no caller-supplied authorization/pass report.

Use existing CSV remaining-budget precharge before appending (32MiB per companion,192MiB total release). The new renderer may reuse bounded internal CSV helpers; it does not require a public new standalone kind. Validate complete output before final publication. Publication remains one staged NEW root with immutable source native protection, no replacement, no partial final directory, bounded workers and cleanup on refusal.

## Provenance and manifest compatibility

Version3 release manifest adds optional `populated_assembly` summary with fixed BOM/placement paths, included `component_count` and `component_ids_sha256`. It is absent in v1/v2, preserving their manifest bytes. Preserve existing calibrated summary and relative-file byte lengths; no added member in that older summary. Version3 generic/checked packages contain13/14 files respectively.

Digest contract: SHA256 over ASCII domain prefix `viamind-populated-components-v1` followed by one zero byte, then each included stable UUID's16 canonical network-order bytes in ascending UUID order. Use the existing workspace sha2 dependency (already used by the engine; MIT/Apache-2.0) with a small explicit fab dependency/lock update; do not implement a separate hash algorithm. Stream over already ordered borrowed IDs; do not allocate a variable-sized ID array or report all100000 IDs. The digest describes the identity subset, not component content/security/vendor acceptance. The existing manifest file map records byte lengths, not content hashes. Complete exported cursor retains source-session provenance; verification reports may independently hash files, without claiming those hashes are part of the release manifest. Independent tests/probes derive the digest separately from CSV IDs and compare both sets and the manifest.

Manifest exclusions must explicitly distinguish old all-component inventories and the two matched populated generic companions; unavailable source identifiers, generic top-view marker limitations, all-mounting versus manufacturer filters, connectivity/full-DFM/procurement/vendor acceptance remain disclosed. Existing fabrication manifest bytes remain unchanged.

## Shared user/agent surfaces

Use existing typed saved-job config/save/load/run through display-free CLI, MCP and retained single async desktop worker. Add an explicit matched populated BOM checkbox available only when calibrated companion is selected; explain both fields must be saved to a NEW descriptor. Turning off calibrated placement clears the local populated-BOM selection so the form returns to an existing valid descriptor; this is form state only. Loading version3 restores both selections. Pending inputs are disabled and hidden polling/error recovery remain intact. Expose readable included count/matched-set digest in scrollable result; do not claim generic inventories are now filtered.

MCP schema/description and canonical reference must be regenerated from the unique verified complete binary. Explicit read-only admission continues to observe settings but excludes run/save/publication; no bypass.

## Required verification before submission

- Public-source spec independently approved before Rust changes; version/default/snapshot/digest contracts reviewed.
- Golden v1/v2 settings/manifests/payload compatibility; strict absent/false/null/duplicate/unknown combinations, included invalid/overbudget sourcing before clone, excluded huge/invalid sourcing and unrelated-sheet omission, empty population, mixed mounting/side, missing calibration and atomic failure.
- One captured snapshot remains stable after source edits; exactly one permit, two-slot busy and canceled async waiter cannot release a live worker. Same captured rules/central DRC cannot be weakened by saved baselines.
- Independent CSV parser verifies Unicode/quotes/commas/newline values and both exact ID sets; separately compute digest; preserve calibrated CSV bytes and all old inventories.
- Actual shared two-client MCP/CLI/compact desktop save/load/run/count/error/newer-edit parity, NEW collisions, each process's own native path and full session invariance. Readable self-closing native inspection with explicit provenance.
- Four required Linux gates, canonical current tool catalog/workflow and fresh exact-head combined CI. Root formal review before merge; story-level full manufacturing/vendor acceptance remains open.
