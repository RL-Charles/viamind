# Explicit schematic buses verification

T-0127 adds explicit graphical bus declarations and bus entries that are kept separate from
ordinary scalar wires and labels. A bus or entry graphic never creates a connection. Unfolding a
member creates one ordinary cardinal wire and one same-sheet Local member label through the shared
engine command; the preview proves the complete source/candidate original-UUID partition, label
names and scopes, and refuses incidental contacts.

The durable evidence index is `docs/verification/t0127/master.json`; run
`python3 docs/verification/t0127/verify-artifacts.py` (add `--retained` on the machine that holds
the retained binaries and native P6/P7 channels).

## Contract and provenance

- Approved public contract: `docs/specs/schematic-buses.md` (pre-Rust approval `bf14e79a`).
- Implementation began on foundation `4551f119` (NPTH model foundation `c404973c` plus merged T125).
  Only the twelve own commits `4551f119..56b75f93` were transplanted onto final main `c92d3ae8`
  (merged T121 NPTH consumers); the pre-transplant tip is tagged
  `verification/t0127/pre-transplant-56b75f93`.
- Native/event feature 16 composes with aperture 14 and NPTH 15 by maximum; empty bus maps stay
  omitted, so older projects keep their bytes.

## Integration with final NPTH consumers

- `viamind_doc::footprint_holes::validate_physical_identities` and the engine's hole identity set
  (`part_holes::identities`) include bus and entry UUIDs. Physical consumers, `set_part_holes` and
  footprint reuse refuse a hole that reuses a bus/entry identity, and fresh hole IDs avoid them.
- Connected drag captures, the combined 100000-identity schematic budget, annotation, PCB branch
  and synchronization budgets count bus and entry rows.

## Output defaults

Selected-sheet SVG/PNG views add a `buses` layer. On a sheet without explicit buses or entries
that layer has no content and is dropped before metadata, and zero bus counts and the bus
exclusion note are omitted. Default output for every pre-bus sheet is therefore byte-identical to
the pre-bus renderer; `crates/render/tests/schematic_buses.rs` compares against an SVG exported by
the retained main binary (`fixtures/views/legacy-oracle/schematic-default-c92d3ae8.svg`).

## Results (source fb05911c)

| Check | Result |
| --- | --- |
| fmt, strict all-target Clippy, workspace build | pass (`docs/verification/t0127/final/final3.log`) |
| Workspace tests | 1104 passed, 0 failed, 1 ignored (246 suites) |
| MCP catalog / workflow | exact, 127 tools / 18 checks |
| Production stdio (Live, Approval, ReadOnly) | 38 checks pass |
| Desktop HTTP, two initialized same-name clients, port 7964 | 53 checks pass (stdio + 15 HTTP) |
| Native WGPU captures | 1600x1000 and 640x400, original RGBA retained |

Earlier failed runs (final1 inherited-test failures, native helper failures before capture) are kept
in the index with their corrections.

## Verification limits

No implicit bus conductivity, aliases, hierarchy or hierarchical bus ports, bus ERC/DRC, PCB
continuity, vendor acceptance, OS input or frame cadence is claimed. Native captures use the
production `ViamindApp` prepared with local egui Harness inputs and an actual eframe WGPU
screenshot; they are not OS-input evidence.
