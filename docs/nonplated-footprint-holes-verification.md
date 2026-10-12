# Explicit nonplated footprint hole verification

T-0121 implements the independently approved [public contract](specs/nonplated-footprint-holes.md):
bounded explicit circular nonplated (NPTH) footprint holes with shared drill geometry.
The earlier [foundation record](nonplated-hole-foundation-verification.md) covers the model phase.
No manufacturing, DFM or minimum-drill acceptance is inferred.

## What is supported

- Optional `holes` on placed parts and detached footprints. Absent/null is Legacy and is omitted
  on save; `Some([])` is retained as deliberate empty intent. At most 256 holes per owner, ASCII keys
  of 1..64 bytes, positive diameters up to 100 mm, and checked doubled local/world extents.
  Cardinal poses are exact; other poses quantize the computed center to the nearest nanometre.
- Hole UUIDs share the global project identity namespace (100000 identities, 2 MiB of keys) and are
  checked before any copy or index. Native/log version 15 and library version 4 compose with the
  existing versions; old files without holes keep their bytes.
- `set_part_holes` requires the exact project, full initial cursor, exact Part and a required
  nullable `holes` member. It is one atomic, undoable delta. Applying a footprint replaces holes by
  key: reused keys keep their UUID, new keys get fresh globally checked UUIDs, and older libraries
  restore Legacy. Creating a linked component refuses explicit holes so drills are never lost
  silently.
- The placed and detached desktop editors share one hole editor with Legacy/Explicit modes, exact
  local-millimetre fields, Apply/Reload/Cancel and stale-cursor refusal. Detached Save NEW never
  applies to the design.
- DRC, walkaround, copper continuity and unrouted analysis treat holes as physical, nonelectrical
  obstacles on both sides. Overlapping or unlocated holes make continuity explicitly unknown.
- Fabrication needs explicit `include_npth: true`. It writes a separate `nonplated-through.xnc` with
  `NonPlated,1,2,NPTH` attributes and keeps existing plated/copper bytes unchanged. Output job 6
  requires the opt-in; checked releases count every physical drill. The release inspector parses
  NPTH and plated drills independently.

## Integration with main

The own-only range `8f5af1d6..30330418` (25 commits) was rebased onto main `88c3ec12`, which adds
T-0125 connected schematic drag and T-0120 aperture integration. The pre-rebase tip is kept as tag
`verification/t0121/pre-transplant-30330418`. `git range-diff` shows 23 identical commits. Two
differ only in merge context: the additive `UiAction` registration and a neighbouring test module.

Three follow-up commits finish the integration:

| Commit | Change |
| --- | --- |
| `0b3b377f` | Connected drag's identity set and `schematic_budget::project` count authored holes; new drag Part fixtures use `holes: None` |
| `9f900007` | A hole editor test now edits through `UiAction` instead of calling the engine directly (`command_parity` gate) |
| `02305397` | The aperture compact test now matches the pinned Save button by its own painted frame, not a scrolled hole row beneath it |

## Results at `023053973d8e1932a5339deafb49fbc66ff61e3d`

The [full Linux gate log](verification/t0121/final/final3.log) shows these results:

- fmt check, strict workspace/all-target Clippy, workspace tests (**1072 passed, 0 failed,
  1 ignored**, 242 suites) and workspace build all pass.
- Rust inputs were touched under the shared mutex before the gates and again just before the normal
  application build.
- The application SHA256 is `ee8ca4dfb608eb28012fae196a872929b77e116029d089306734f970fa95859a`.
- The inspection host SHA256 is `5e7bcb34ff1d941b6be82d96553cca1e3f5c585d86a50ca974a493e8c8dc3606`.
- The 121-tool generated catalog matches `docs/generated` exactly. There are 18 agent-workflow
  checks and 13 Python tests.
- 38 actual headless stdio checks pass, including the restricted-writer protocol denial and the
  no-display CLI.
- 5 actual checked-callback checks pass. Overlapping NPTH is refused before publication, the
  complete job 6 succeeds, and the independent reader sees 1 NPTH and 2 plated drills.

Two earlier full attempts at intermediate heads failed, and their logs are kept: `final1` hit
`command_parity` and `final2` hit the aperture clip heuristic. Both are fixed by the commits above.

## Rebased final source

After T-0126 merged, the branch was rebased onto main `0e8616b5`. T-0126 and T-0121 change no
common files, and all 30 commits are identical in `git range-diff`. The gated pre-rebase source and
evidence are tagged `verification/t0121/native-source` and `verification/t0121/native-evidence`.

The rebased source `a530c641afa2ee0ea4fabd158182b0fe7315dc4a` passed the full gate again
([log](verification/t0121/final/rebased-a530c641/final4.log)):

- fmt check, strict Clippy, workspace tests (**1074 passed, 0 failed, 1 ignored**) and a retouched
  workspace build all pass.
- The application SHA256 is `b7ff6506727bc300ce8afb8998f492a8d3d6fb4e3070a1d416f49e2ed1a011dc`.
- `docs/generated` matches the catalog exactly, and the 18 workflow checks, 38 stdio checks and
  5 checked-callback checks pass.

The native captures and two-client HTTP checks below were not repeated. They remain bound to
`02305397`.

## Actual desktop and native evidence

The production application was started on reserved port 7963 through the `cua-driver launch_app`
owned wrapper. Its `/proc` executable hash equals the gated binary.
[40 checks with two initialized HTTP clients](verification/t0121/final/http-02305397/production.json)
pass. They cover the shared catalog, peer visibility, stale refusal, undo/redo, keyed library
reuse, the NPTH opt-in and saved-release inspection. The Cua window capture only shows launch
provenance.

Four inspection hosts were launched through owned wrappers. Each self-closed after one native WGPU
Screenshot event, with the full session and document status unchanged:

- `placed`: the 1600x1100 placed hole editor, with exact fields and reachable Apply/Reload/Cancel.
- `compact`: the 640x400 placed editor, with pinned actions and the diameter field inside the
  effective clip.
- `board`: the 1600x1100 native board, with the mechanical layer and the hole on the bottom part.
- `completed`: the 640x400 detached Save NEW notice. The saved library equals the editor definition.

These hosts prepare the production UI with actual egui Harness input and transfer its state and
memory to eframe. This is not an OS-input or frame-cadence claim, and native rendering is not a
boolean copper-clipping claim.

The [master](verification/t0121/master.json) indexes every receipt, PNG and retained raw channel
and binary. Run `PYTHONDONTWRITEBYTECODE=1 python3 docs/verification/t0121/verify-artifacts.py --retained`
for the local audit, or omit `--retained` to check only the repository.

## Limits

Only circular NPTH is supported, with no slots, ovals or plated mechanical holes. No manufacturer
minimum-drill rule is added. Exact-head CI and independent review are required before merge, and
human sign-off is separate.
