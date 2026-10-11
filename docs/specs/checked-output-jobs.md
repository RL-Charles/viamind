# Checked manufacturing output jobs

## Public behavior sources

- KiCad 9.0 command-line documentation, https://docs.kicad.org/9.0/en/cli/cli.html, documents predefined job execution with stop-on-error, machine-readable JSON PCB DRC reports and a failing process exit code when violations exist. Consulted 2026-10-11. These are behavioral references; no source code, file format compatibility or UI assets are copied.
- Ucamco official Gerber documentation, https://www.ucamco.com/en/gerber, defines manufacturing image exchange independently of the author's design checks. Existing Viamind Gerber/XNC output remains the supported subset specified in [fabrication-gerber.md](fabrication-gerber.md); this increment adds a scoped validation gate, not new image syntax.

## Purpose and exclusions

Add a detached `checked_outer_2layer` output-job profile that refuses publication unless scoped DRC completes without violations over every exported explicit pad, track and through-via. The existing `generic_outer_2layer` profile, canonical settings and output bytes remain unchanged. No vendor acceptance, schematic/board parity, physical electrical continuity, complete DFM, routing repair, sourcing/population changes or assembly centroid inference is claimed.

Unimplemented categories that cannot exist in the current supported explicit model remain named exclusions. The permanent `checks_not_performed` list does not itself make the supported subset fail. Actual excluded components, incomplete enumeration, truncated reports, missing outline, uncovered exported geometry or any scoped violation do fail.

## Detached strict settings

Keep the existing format/version/name/profile/mask fields. The new profile requires one explicit `checks` object; generic jobs reject it. Unknown job/check fields fail; invalid profile/check combinations fail. Existing generic JSON omits the absent object and remains byte identical.

The checks object declares all six current distance baselines in millimetres (copper, edge, via drill, annular width, hole-to-hole and hole-to-copper), each finite within 0..100, plus max_violations 1..10000. They are request baselines. Persisted document exact named-net minima must be captured on the same owned board and centrally maxed with request baselines by shared DRC; a saved request cannot weaken a document minimum. Track-width minima from document rules apply even if the saved profile has no independent width baseline. Preserve both requested and effective/document rule provenance in the validation report. Saved checks use the six existing DrcInput distance field names plus max_violations; any future document minimum track-width rule is enforced centrally without adding an implicit preferred width.

Existing 64 KiB config, 256-byte name and 4096-byte path bounds remain. Safe Linux load uses NOFOLLOW|NONBLOCK and opened-fd regular-file checking. Config remains detached: save/load/run do not change project, native dirty state, selection, history or event cursor.

The current T-0095 model has no persisted named-net policy. No such policy is
inferred or claimed by this increment; the policy integration contract above
belongs to the later rules feature and must preserve this same-snapshot boundary.

## One immutable snapshot and worker

Load/validate bounded settings outside the engine mutex. Under one engine lock: validate full expected stream/sequence first, preflight borrowed primitive/header/net/rule budgets before copying strings, acquire one nonblocking manufacturing permit, capture one board-only owned project together with project ID, full cursor and native source path. Ignore unrelated schematic and unused sourcing strings. Retain bounded document rules used by DRC; no second board clone or admission permit.

One retained worker owns snapshot/permit from capture through validation, render, stage, sync, publication or error. Cancellation of an async caller does not release admission while its blocking worker remains alive. No file I/O, DRC, rendering or awaiting under engine lock. Preserve existing at-most-two process manufacturing workers and purpose-specific primitive/text limits. A stale request rejects before admission.

## Shared check and completeness contract

Shared engine validation receives the exact immutable project owned by the manufacturing snapshot. It runs the existing production DRC with the saved baselines and document minima. Before any release files are published, require:

- Native-supported polygon outline with explicit outer ring and every cutout checked.
- No legacy preview or other excluded component; explicit pads may be empty only where no exported pad exists.
- checked_pads equals the sum of all explicit exported pads; checked_tracks equals all captured tracks; checked_vias and checked_holes each equal all captured through vias. Validate counts against borrowed model geometry, not user-supplied counts.
- excluded_part_count == 0, excluded detail truncation false, truncated false, incomplete_reason absent and zero violations.
- Shared exporter still rejects unsupported/invalid geometry, layers or numeric data. No successful DRC result overrides exporter rejection.

Unassigned copper remains subject to existing conservative clearance checks and is reported; no net completeness claim. Empty copper is allowed only with an explicit valid outline, and reports zero checked geometry without claiming a populated design.

## Crate boundary

Keep fab independent of engine/DRC. Extend its private-snapshot write boundary with a mandatory checked-profile callback invoked on the exact private immutable &Project before render. Generic write rejects checked profiles unless this boundary is used. One shared engine helper takes the production DRC input, verifies the completeness/count conditions and returns a bounded serializable validation artifact. Small adapters convert the shared saved-settings fields to that typed input; CLI/MCP/desktop invoke the same helper. The callback may receive bounded serialized rule settings and return bounded serialized report data, allowing the shared engine helper to deserialize its existing typed DrcInput without a new dependency edge. Do not introduce a fab->engine or fab->DRC dependency. All application call sites must bind the production helper; no external pass flag authorizes release.

The callback and snapshot stay synchronous inside the retained blocking worker. Bounds on the validation JSON are charged against remaining release bytes before serializing/adding files. No caller-supplied report, pass flag or externally generated DRC file can authorize a checked release.

## NEW staged release

Successful checked release contains all existing generic release files unchanged plus fixed `validation/drc.json`; job.json reflects the new settings and release-manifest.json lists the new file. Validation artifact contains schema/version, captured project ID/full cursor, requested baselines, document/effective rule provenance, covered counts, the complete scoped DRC report and honest exclusions. No timestamps or output destination influence geometry file bytes. Existing fabrication manifest remains byte identical.

Keep 192 MiB aggregate release cap including validation bytes and complete fixed file inventory. Build and fsync a private staging tree; sync subdirectories; publish one NEW root with Linux NOREPLACE rename. Preserve native source path and existing destination files/directories/symlinks. Any validation/render/write/sync/collision failure publishes no release and removes its own stage. No overwrite option or multi-directory partial publication. Return complete exported cursor and readable validation summary; newer desktop edits after capture are allowed and clearly identified.

## CLI, MCP and desktop

Use the existing detached save/load/run requests and retained output-job dialog. Add explicit profile selection and bounded check fields with scrollable compact controls. Disable relevant pending inputs, poll hidden worker completion, retain errors/recovery and structured results. Shared schemas/catalog describe checked coverage and exclusions. MCP malformed/unsupported requests reject atomically. CLI validation failure exits nonzero and does not produce a release directory.

## Required evidence

Meaningful tests: old canonical config and all generic bytes unchanged; strict profile/check parsing and bounds; clean checked release; each representative DRC violation refuses publication; missing outline/legacy exclusions/report truncation/candidate exhaustion/count mismatch reject; document minima cannot be weakened; one capture+permit/cancellation ownership; unchanged complete session/source native; stale two-client requests reject before admission; edits after capture do not alter validation/output; NEW collision/symlink/race/write failure cleanup; validation JSON inventories and bytes within cap; CLI/MCP/desktop parity; actual compact scroll/input/error/recovery.

All four Linux gates plus fresh exact-head CI. Retain production CLI/actual HTTP agent verification with exact binary/source hashes and durable reports. Readable native inspection must label its real capture route and wrapper provenance; no vendor certification or frame-cadence claim.
