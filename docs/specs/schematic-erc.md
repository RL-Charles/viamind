# Bounded configurable native pin ERC

## Public behavior reference

Primary source: [KiCad 9 Schematic Editor manual, Electrical rules checking and ERC Configuration](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#electrical-rules-checking). Its documented model includes configurable allowed/warning/error pin pairs, unconnected/undriven inputs and power inputs without power-output sources. The native policy below is deliberately explicit and independently designed; it does not claim to reproduce another application's whole default matrix. No implementation code or proprietary artifacts were consulted.

## Shared snapshot and scope

`run_erc` is read-only, takes per-request policy and returns a report with project UUID/source cursor, effective symmetric compatibility matrix, stable pin/symbol/sheet/net references, deterministic issue IDs and explicit coverage. It uses one bounded exact [native schematic connectivity](schematic-connectivity.md) snapshot, including transformed pin endpoints and local/global/power label scope. Graph calculation and ERC run outside the engine lock. Capture/geometry/identity/ambiguous-label errors reject the whole request without partial nets or a false clean report.

No design, undo/redo, dirty state, selection or session event changes occur. The policy is transient request data; it is not a project rule or an undoable model edit. It can therefore be used consistently from desktop code and MCP. Initial delivery is engine/MCP only.

## Deliberate default policy

Pair rules are symmetric. All 66 unordered pairs of the 11 native pin types have an effective severity, with these defaults applied in priority order:

1. Any pair containing `no_connect`: **error**.
2. Any pair containing `unspecified`: **warning**.
3. Pairs whose two pins are each `output` or `power_output`: **error**.
4. All remaining pairs: **allow**. In particular tri-state/bidirectional and open-collector/emitter pairs are allowed; their enable/pull circuitry is not analyzed.

`compatibility` overrides individual unordered pairs with allow/warning/error. At most 66 entries; duplicate or reversed duplicate pairs reject, even if their severities agree. This prevents order-dependent policy. Effective defaults and overrides are returned, including allowed pairs. No pair involving an isolated single pin is invented.

Single-pin checks have independently configurable severities (allow disables; default **error**, except `no_connect_attached` **warning**):

* `unconnected_input`: an `input` has no other pin in its component. A dangling wire/label alone does not supply a peer pin.
* `undriven_input`: an `input` has no other pin of type output, bidirectional, tri_state, open_collector, open_emitter, power_output or passive. Passive is a native signal-driver allowance following the public behavioral reference; it never supplies power.
* `missing_power`: a `power_input` component has no `power_output` pin. A power label names/joins nets; it is not a power source. This check reports each affected input pin.
* `no_connect_attached`: a `no_connect` pin has any other graph member (including a wire/label/junction). Pin pair checks also apply, so two different supported conditions may report on the same pin.

An isolated input can receive both unconnected and undriven errors. Unspecified pin types are counted in coverage and only participate in the documented pair policy; the checker does not infer pin types from names. Connection across a passive component body is never inferred.

## Bounds, ordering and completeness

The existing graph limits apply: 1000 sheets, 10000 total sheets/symbols/pins/wires/junctions/labels, 2MiB text, 1000000 spatial contact candidates. Pin metadata is captured with that same graph, not read from a second live snapshot. ERC visits at most 250000 unordered pin pairs and returns at most `max_violations` (default1000, 1..10000). Pair counting includes allowed/ignored pairs. Both limits yield `complete=false`, an explicit stop reason and an honest deterministic prefix; exceeding a graph limit returns an error with no report.

Components are visited in stable graph ID order. Within each net, single-pin checks visit pin UUID order in the order above, then pairs in lexicographic pin order. Issues identify check, net and one/two persisted pin IDs. Counts distinguish visited nets/evaluated pins/tested pairs and returned violations. A report complete for these checks always states `full_erc=false` and lists exclusions: hierarchy/buses/hidden power pins, no-connect markers and power flags absent from the native model, multi-unit/shared pins, simulation/board annotation, net classes, off-grid/wire/label checks and electrical correctness beyond the selected policy.

## Verification

Test default matrix symmetry/completeness, all pair override severities, reversed duplicate rejection, exact crossing/junction topology, isolated vs wired inputs, signal/passive drivers vs actual power outputs, explicit power label not driving, no-connect attachment, unspecified coverage, deterministic stable references, snapshot immutability, graph failures and both work/result limits. Actual two-client MCP discovery/configuration/edits/undo and production headless checks must agree with native engine results. The regulator fixture deliberately reports missing power where its source is unmodeled; it must never be used as a fake all-clear example.
