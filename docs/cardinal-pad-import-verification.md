# Cardinal rectangular pad import verification

T-0094 extends the existing bounded PCB importer and detached footprint
converter with exact cardinal rectangular pad geometry. The
[public-source specification](specs/cardinal-pad-import.md) preceded the code
and was independently reviewed before implementation. Original fixtures were
authored for this project; no external EDA source, libraries or assets were used.

## Exact scope and geometry

Containing footprints still require front copper and zero rotation. Pad angles
are exact ASCII integer decimals, optionally negative and with an all-zero
fraction, within inclusive −360…360 degrees. Only multiples of 90 are accepted.
There is no floating-point angle calculation, trigonometry, rounding, reflection
or combined footprint/pad transform inference. Euclidean normalization maps
signed equivalents to 0/90/180/270. Centers are unchanged; only 90/270 swap the
two integer Nm dimensions. Native pads have no angle field.

| Original fixture pad | Source angle | Local center (Nm) | Source size (Nm) | Native size (Nm) |
| --- | --- | --- | --- | --- |
| PCB and detached pad 1 | 90 | −500003, 250001 | 800001 × 1200003 | 1200003 × 800001 |
| PCB pad 2 | −180 | 500003, 250001 | 800001 × 1200003 | 800001 × 1200003 |
| Detached pad 2 | −270 | 750007, −125003 | 1000001 × 600003 | 600003 × 1000001 |

The public-parser regression uses an independent doubled-Nm corner rotation
oracle, retaining exact half-Nm corners of odd-size rectangles. Its matrix
includes absent angles, signed cardinal equivalents, both ±360 bounds, zero
fractions and leading zeros. Rejections cover noncardinal/fractional values,
malformed grammar, exponents, plus signs, overflow and oversized tokens. Both
containing rotation/back-side refusals and source DNP refusal remain tested;
assembly metadata is not inferred.

Every parsed `at` and `size` list discloses bounded source/canonical values,
unchanged center and exact dimension normalization. PCB pad IDs/nets persist.
Detached source UUIDs remain report-only, meaningful source nets block
conversion, and applying a resulting library is a separate guarded undoable
action retaining native IDs/nets by number. Full fidelity remains false.

## Frozen Linux gates and production binary

All four Linux gates passed on
`1cf3724b7a8a8a4ff739917929a94bf30ba27be5`, based on
`cc1813f2c734eb808d702002da4bcc2c93bf6bf3`:

- `cargo fmt --all -- --check`;
- strict workspace/all-target Clippy with warnings denied;
- **641 workspace tests passed, zero failed, one existing ignored**, including
  doctests;
- workspace build and both native inspection example builds.

The full sequence held `/tmp/viamind-workspace-gates.lock` through doctests and
unique binary copies. It refreshed workspace Rust mtimes and used locked offline
dependencies, two build jobs, incremental compilation disabled and home-cache
temporary files. An additional focused two-test geometry matrix and four-test
actual desktop suite passed first; they are excluded from the 641 workspace
count. The [raw log](verification/t0094/final-full.log) records the source,
commands' output and exact retained executable hashes. Production SHA256:
`ec8a39ef2c0c0b427bc38a6948759d2a0fb5fe274e166fa812c74e0b0cc48427`.
This is a debug executable with debug information stripped.

## Real production MCP

The two retained checkers ran against this exact executable without a display
over stdio and against its actual desktop server on port 7920. Desktop PID
1274306 was independently matched through `/proc/1274306/exe`; the listening
socket belonged to that PID. Each HTTP checker initializes two independent
MCP clients.

| Path | Headless stdio | Actual desktop HTTP |
| --- | --- | --- |
| PCB import | 21 checks passed | 20 checks passed |
| Detached footprint conversion | 21 checks passed | 20 checks passed |

The 82 assertions include exact odd-Nm dimensions/centers, original PCB IDs and
VIN/GND nets, deterministic source reports, detached identity/net semantics,
45/450/90.000000001 refusal without mutation, source digest changes, active native
path protection, dirty-peer/full-cursor guards, NEW-only publication,
same-project reopen, native save and independent undoable number-based reuse.
The [four reports](verification/t0094-linux-native.json) retain exhaustive
fidelity results and exact source hashes. Existing engine and real HTTP
integration tests also exercise these original cardinal fixtures.

The asymmetric PCB intentionally leaves **4 Nm** between differently assigned
pad rectangles. Native DRC reports that actual clearance violation against the
200000 Nm rule. It checks two pads, two tracks, one via, one hole and the outline,
without truncation or excluded parts; unperformed checks remain explicit.
Import success does not imply clearance, manufacturing acceptance or equivalence
to the source tool's rules. The full DRC result is retained in both PCB reports.

## Actual desktop input and readable native captures

Four real egui input tests cover menu entry, dirty-session and live wire-draft
preservation, exact preview dimensions, compact scrolling to Create and errors,
path/project/source guard refusal, and wheel scrolling to the normalization
row with complete session equality. Shared UI actions retain MCP parity.

The retained inspection examples prepare the real production `ViamindApp`
through actual egui input and then render it in fixed native WGPU windows. MCP
is disabled in these separate hosts. Complete `SessionSnapshot` equality is
asserted on every native UI callback, and prepared normalization/Create bounds
are asserted inside the actual native content rectangle.

| Capture | Pixels | What was visually checked |
| --- | --- | --- |
| [Normalization report](verification/t0094/normalization-native.png) | 1600 × 1000 | Source 90 / canonical 90, unchanged center, explicit incomplete fidelity |
| [Compact form](verification/t0094/compact-native.png) | 640 × 400 | Create and preservation notice fully reachable after actual scrolling |
| [Completed conversion](verification/t0094/completed-native.png) | 1600 × 1000 | Created path, two pads, no project application and unchanged session notice |
| [Imported board](verification/t0094/board-native.png) | 1600 × 1000 | Orthogonal pad rectangles, labelled component, both copper sides, via and cutout |

The inherited report renderer truncates long reasons and exposes their complete
text as a hover tooltip. The normalization image consequently clips the end of
the width/height explanation; it is not claimed to show the entire reason.
The complete reason and both exact swaps are checked in the actual input test,
native production checks and retained JSON reports. Completed controls/path and
compact controls are readable. No unrelated renderer redesign was added.

Exact own-PID Cua launch/list-window calls returned no window targets, so no
valid compositor capture target could be used. This is not a successful Cua
screenshot or OS-input claim. The separately authorized native Screenshot
fallback reads actual opaque WGPU framebuffer pixels as an exclusive new P6
file. The [retained encoder](verification/t0094/encode-native-ppm.py) losslessly
encodes PNG and verifies decompressed RGB equality. Source/output/pixel hashes,
actual PIDs, executable hashes, viewport/control bounds, launch records and
the four native logs are retained in the master and process evidence.
The historical log field `native_frames: 11` counts **UI callbacks/passes**,
not physical or presented frames. There is no frame-cadence claim.

## Reproduction and remaining scope

```sh
python3 scripts/verify-pcb-import-native.py \
  --binary /absolute/path/to/verified/viamind \
  --fixture fixtures/import/original-cardinal-pads.kicad_pcb --cardinal \
  --report /new/path/pcb-stdio.json
python3 scripts/verify-footprint-conversion-native.py \
  --binary /absolute/path/to/verified/viamind \
  --fixture fixtures/import/original-cardinal-pads.kicad_mod --cardinal \
  --report /new/path/footprint-stdio.json
```

For owned desktop proof add `--url http://127.0.0.1:7920/mcp`, after verifying
that the exact task-owned executable owns that socket. These checkers create
and edit temporary native sessions/files; use an owned verification instance.
The native example argument lists are retained verbatim in
[native-processes.json](verification/t0094/native-processes.json). Inspection
files/output paths must be new. Encoding uses only Python's standard library.

[Master evidence](verification/t0094-linux-native.json) indexes all artifacts.
Historical product/inspection source and executable hashes remain unchanged by
documentation commits or clean integration rebases. Final combined artifact-head
CI remains a merge gate. Neither full source-format/corpus fidelity nor bottom
footprints, arbitrary rotations, shapes, mask/paste/artwork, assembly defaults or
manufacturing equivalence are claimed. S-079/S-083 remain broader open work.

## Final integration

The ticket rebased onto merged BOM-template, AppImage and cardinal-cleanup main
`bff6851ae4a8c35a882c7228f1fd5dd6442896ec`. Seven ticket commits compare equal
in `git range-diff`. One MCP registry context conflict was resolved by retaining
both the merged `plan_cardinal_track_cleanup` entry and the cardinal PCB preview
description. Every T-0094 owned file remains byte-identical to `61eb879`, apart
from additive main changes in `EngineHost`; both cardinal preview definitions
are also byte-identical. No parser, geometry, guard or UI behavior changed.
Formatting and whitespace checks pass; fresh combined exact-head CI gates
merge. Historical binary/native evidence above keeps its original source hashes.
No duplicate local workspace or native run was performed for this integration.
