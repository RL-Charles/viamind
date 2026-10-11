# Native schematic connectivity verification

T-0061 adds a bounded read-only graph query; it does not perform ERC or board annotation. See the [behavior specification](specs/schematic-connectivity.md).

## Linux checks

At feature commit `a10f514`, Linux formatting, strict workspace all-target Clippy, complete workspace tests and workspace build passed: **310 passed, zero failed, one existing ignored**, including doctests. Eight focused geometry/scope/bounds tests and the actual two-client HTTP test passed. The final rebase onto `a480fd5` preserved graph implementation and tests; the only conflict concatenated the independent SVG and schematic connectivity README sections. Fresh PR CI verifies the combined source.

Reproduce the tracked protocol test with:

```sh
cargo test -p viamind-mcp --test schematic_connectivity
cargo test -p viamind-engine schematic_connectivity
```

## Actual production binary

The verified debug `viamind` binary was copied while holding the shared build lock, then driven through its real `--headless` stdio MCP transport with DISPLAY/WAYLAND_DISPLAY unset. Discovery, immutable repeated queries, crossing/junction topology, undo/redo, explicit global/power scope, document replacement stream refresh and clean exit passed. Results retain snapshot cursors and stable persisted identities.

The original [regulator fixture](../fixtures/schematic/regulator.viamind.json) from merged `a480fd5` was opened through `open_project` and queried through `query_schematic_connectivity`. Fixture SHA256: `1826f61bdee38eb41779ee1e76143bad96249b0803d7702b9edf8798c87318e2`.

| Net | Pins | Members represented |
| --- | ---: | --- |
| VIN | 2 | regulator input and input capacitor |
| 3V3 | 2 | regulator output and output capacitor |
| GND | 3 | regulator ground and both capacitor ground pins |

Exactly three components cover all seven pins. Repeated query returns the same graph and leaves the complete session unchanged.

A separate production MCP case created 1000 completely overlapping wires. The query refused after its hard 1000000 candidate-contact budget, returned the actionable simplify-geometry error without partial structured nets, and preserved complete session and dirty state. Debug elapsed was 981.062 ms. A sparse 1000-wire fixture used exactly 2000 candidates and took 65.623 ms. These are observational debug samples, not a desktop latency or full-project performance guarantee.
