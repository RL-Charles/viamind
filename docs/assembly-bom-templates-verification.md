# Assembly BOM template verification

T-0092 adds populated-only Catalog and Assembly BOM layouts. This evidence covers
their explicit local input contract and shared implementation; it does not prove
manufacturer upload acceptance, catalog stock, mounting type or assembly placement.

## Production application

The retained production executable was built from
`64a677564d9afecfd380a90e91a3f0137e25fcd4`, based on main `cc1813f2c734eb808d702002da4bcc2c93bf6bf3`.
Its SHA-256 and running-process SHA-256 match the [master record](verification/t0092-master.json).
The four Linux gates passed, with 646 workspace tests passed, none failed and one
ignored. The [exact source CI run](https://github.com/RL-Charles/viamind/actions/runs/38139766607)
also passed all four gates. The later evidence commit changes documentation and
the separate inspection harness only.

The unchanged production application opened a copy of the tracked
[asymmetric fixture](../fixtures/assembly/bom-templates.viamind.json), with HTTP MCP
on port 7919. Two initialized agent sessions and the production no-display CLI
passed [31 checks](verification/t0092-production-report.json). Independent Python
CSV parsing checked the exact headers, Unicode/quote escaping, preserved `0603`
package, synthetic catalog identifier and empty uninferred cells. Both templates
contained one populated component; the explicit do-not-populate component was
excluded. CLI and HTTP output bytes matched.
The exact synthetic [Catalog CSV](verification/t0092-catalog_bom.csv) and
[Assembly CSV](verification/t0092-assembly_bom.csv) are retained with the evidence.

The probe also checked NEW collisions, explicit overwrite, native-file protection,
unchanged complete session state during accepted exports, stale guards, rejected
required fields and undo. Three metadata edits and three undos advanced the final
cursor to sequence 6 while restoring the exact original project and clean native
status. Successful exports themselves created no events. Previous BOM, placement,
sourcing BOM, seven fabrication files and ten saved-job/config files matched the
retained T-0087 binary byte for byte. The saved-job release manifest's independent
session cursor was excluded from that cross-session comparison.

## Desktop inspection and input

The separate [inspection harness](verification/t0092-native-inspection.rs) embeds
the production `ViamindApp` in a fixed 1600×1000 viewport, opens the export dialog
and injects a documented local egui format-choice click. It starts no MCP listener
and changes no shared model. This viewport and input belong to the harness;
production window resizing and input handling are unchanged.

The [fresh native capture](verification/t0092-native-final.png) is an actual
eframe WGPU framebuffer screenshot with Catalog BOM visibly selected. The
[framebuffer report](verification/t0092-native-framebuffer-report.json) records
two inspection-only pointer events and complete unchanged session state. Opaque
P6 RGB pixels were converted to PNG with ImageMagick; independently decoded RGB
bytes matched exactly. The [Cua attempt](verification/t0092-native-cua-attempt.json)
refused the exact target as stale and produced no screenshot, despite a zero
process exit code. Cua launch metadata returned no windows; separately retained
[compositor metadata](verification/t0092-inspection-compositor-metadata.json)
identified the mapped 1600×1000 Wayland window. No Cua widget-click, operating-system
input or frame-cadence claim is made.
The separate actual 640×400 `egui_kittest` regression selects both new formats,
enters paths, exports through the retained worker, scrolls results, compares bytes
with live MCP, and exercises visible collision failure and recovery.

The harness also accepts an optional NEW PPM capture path. It requests an actual
eframe WGPU framebuffer screenshot after the local format-choice input, checks
opaque pixels and a four-million-pixel cap, writes lossless RGB pixels and reports
complete shared-session equality. This provides a native capture route when Cua
cannot enumerate a new window; it performs no operating-system input.

## Reproduce

Build with the four commands in [AGENTS.md](../AGENTS.md). Copy the fixture into a
scratch directory and launch the production executable with
`VIAMIND_MCP_PORT=7919 viamind --open /scratch/board.viamind.json`, choosing an unused
port. Run the tracked probe with a fresh output directory:

```sh
python3 docs/verification/t0092-bom-template-smoke.py \
  --binary /absolute/path/to/viamind \
  --project /scratch/board.viamind.json \
  --directory /scratch/new-evidence \
  --repo "$PWD" \
  --endpoint http://127.0.0.1:7919/mcp \
  --previous-binary /absolute/path/to/retained-t0087-viamind
```

The probe requires the verified previous executable for its old-format byte
comparisons. Its exact source/hash is in the master record.
For the visual harness, expand `@REPO@` in the tracked
[Cargo manifest template](verification/t0092-native-Cargo.toml.in), copy the harness
to `src/main.rs`, build and run it with a scratch fixture path. Binaries and scratch
output directories are retained locally and are not committed.
Pass a second, new `.ppm` path to obtain the framebuffer capture and JSON report
on stdout. PPM-to-PNG conversion must preserve the exact RGB pixel bytes.

The [public-source specification](specs/assembly-bom-templates.md) and
[usage guide](assembly-bom-templates.md) describe required identifiers, population,
reference rules, budgets and the remaining placement/manufacturing limitations.
