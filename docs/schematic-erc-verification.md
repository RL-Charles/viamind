# Native pin ERC verification

T-0064 implements bounded read-only native pin checks over one exact connectivity snapshot. See the [behavior and default policy](specs/schematic-erc.md). `coverage.full_erc` is always false. No hierarchy, buses, hidden power-pin inference, absent native markers/power flags, board annotation or desktop ERC display is claimed.

## Linux checks

At feature commit `9e75a08`, Linux formatting, strict workspace all-target Clippy, complete workspace tests and workspace build passed: **335 passed, zero failed, one existing ignored**, including doctests. The clean rebase onto `7bac3bd` changes no ERC implementation or tests; fresh PR CI verifies the combined source.

Six focused tests cover the symmetric 66-pair matrix, overrides and invalid policy, supported input/driver/power/no-connect checks, exact crossing/junction topology, immutable deterministic identities, honest result truncation, ambiguous graph rejection and the actual 250000 pin-pair bound. The real two-client HTTP test covers tool discovery, schema, independent per-request policies, source cursors, unchanged history/selection/dirty state, edits, undo and rejected requests.

```sh
cargo test -p viamind-engine erc
cargo test -p viamind-mcp --test erc
```

## Actual production binary

The verified Linux debug `viamind` binary was copied under the shared build lock and driven through real `--headless` stdio MCP with DISPLAY/WAYLAND_DISPLAY unset. Discovery, two coincident output pins, isolated input and power-input checks, the complete effective matrix, deterministic read-only repeats, policy overrides, incomplete result prefixes, undo/redo and clean exit passed.

A separate native request created 710 coincident passive pins and actually evaluated the hard **250000 pin-pair** allowance. The report returned `complete=false`, `stop_reason=pin_pair_limit`, no violations and unchanged session state. Zero returned violations in this incomplete report is not an all-clear.

The original [regulator fixture](../fixtures/schematic/regulator.viamind.json), SHA256 `1826f61bdee38eb41779ee1e76143bad96249b0803d7702b9edf8798c87318e2`, was opened through `open_project` and checked through `run_erc`.

| Supported check | Net | Reason |
| --- | --- | --- |
| Missing power | VIN | Native power-input pin has no power-output source |
| Missing power | GND | Native power-input pin has no power-output source |

The scoped report is complete and `full_erc=false`. Power labels join/name nets and never manufacture power sources. The fixture deliberately models no external power-output source; these two actionable diagnostics are expected.
