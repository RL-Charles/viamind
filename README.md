# Viamind

Open-source PCB design suite in Rust. Clean-room — see [CLEAN_ROOM.md](CLEAN_ROOM.md).

| Crate | Purpose |
| --- | --- |
| `crates/geom` | Geometry, nanometre units, spatial index |
| `crates/doc` | Design model: projects, sheets, boards, rules |
| `crates/engine` | Typed commands, transactions, undo, event log |
| `crates/render` | Canvas rendering |
| `crates/mcp` | Agent harness: MCP server over every engine command |
| `crates/ui-egui` | Desktop UI (egui/eframe) |
| `crates/router` | Interactive and guided routing |
| `crates/drc` | Design rule checking |
| `crates/fab` | Manufacturing outputs |
| `apps/viamind` | Application binary |

Licensed under the [MIT License](LICENSE).

## Try it

```sh
cargo run --release -p viamind          # opens a new design and serves it to agents
claude mcp add --transport http viamind http://127.0.0.1:7878/mcp
```

Ask your agent to place parts (`add_part`, `move_part`, `delete_part`); they appear live in the window.
Undo anything with Ctrl+Z or the Edit menu; agents undo their own changes with the `undo` tool.
`viamind --demo` opens the synthetic 10k-object test scene, and `VIAMIND_MCP_PORT` changes the port.

Click a part to select it, or drag a box (left to right encloses, right to left crosses).
Choose **Edit → Lasso** for freeform selection. Shift adds, Ctrl/Cmd toggles,
Ctrl/Cmd+Shift removes, Ctrl/Cmd+A selects visible parts, and Esc clears or cancels.
Middle-drag and Space+drag pan. Entirely hidden parts are excluded from mouse selection.
Agents share the same selection through `get_selection` and `set_selection`:

```json
{"ids": ["<part ID from get_design>"], "mode": "replace"}
```

Selection preserves design undo/redo. The prototype currently draws common built-in
footprints; schematic capture, routing and library editing are planned features.

## Run an agent without a window

`viamind --headless` serves the full design engine over stdio MCP. It works
without a display and uses the same editing, selection, query and undo tools as
the desktop. Register the built application with your agent client, for example:

```sh
cargo build --release -p viamind
claude mcp add viamind-headless -- "$PWD/target/release/viamind" --headless
```

To work on an existing native project and retain the result, add file options to
the server command:

```sh
viamind --headless --open design.viamind.json --save-as result.viamind.json
```

The agent client speaks MCP on stdin/stdout. Wait for pending tool responses before closing stdin. Closing stdin finishes the session
and writes `--save-as`; diagnostics go to stderr. The input file stays unchanged
unless you explicitly use its path as the output. Without `--save-as`, the
headless session is temporary. `viamind --open design.viamind.json` opens an
existing project in the desktop. See `viamind --help` for options.

Agents can inspect a subset of parts with `query_parts`, combining IDs, exact
fields, text, side and origin bounds, with bounded UUID cursor pages. See
[the MCP guide](crates/mcp/README.md) for examples and the repeatable live smoke test.
