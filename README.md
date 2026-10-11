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

Linux is the initial target. CI builds and tests on Linux; macOS and Windows
support is deferred.

```sh
cargo run --release -p viamind          # opens a new design and serves it to agents
claude mcp add --transport http viamind http://127.0.0.1:7878/mcp
```

Ask your agent to place parts (`add_part`, `move_part`, `delete_part`); they appear live in the window.
Save with **File → Save** (Ctrl+S) or **Save As** (Ctrl+Shift+S). The title marks
unsaved changes, including agent edits. Agents use `save_project` with a native
path, or omit the path to save the active file; `get_document_status` reads its
path and dirty state. A supplied existing destination requires `overwrite: true`.
See [the native save contract](docs/specs/project-saving.md).
Use **File → New** (Ctrl+N) or **Open** (Ctrl+O) to switch projects. New/Open and
closing offer Save, Discard, or Cancel for unsaved changes. Agents use `new_project`
and `open_project`; discarding unsaved work requires `discard_dirty: true` with the
current `expected_cursor` from `get_session_state`. Document replacement resets
history and selection, and observers resynchronize with a new session snapshot.
Use **Edit → Place part** to enter a designator, value, footprint name, position,
rotation and side. Selecting one part shows editable properties in the sidebar;
**Apply properties** commits one undoable edit. **Reload** refreshes an older draft
when an agent has changed that part. Agents use `update_part` with an optional
`expected` part snapshot to guard against stale edits.
Use **Board → Board outline** to define an outer polygon and internal cutouts.
Enter one X,Y pair in mm per line, with blank lines between cutouts. Explicit
outlines stay fixed when parts move; malformed or stale edits are rejected.
Outline-bearing native files use version 2 and require a build with outline support.
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

Agents can follow design and selection changes through `get_session_state` and
`watch_events`. The snapshot includes a cursor that resumes after reconnecting;
watching supports bounded long polling on HTTP and stdio. See the
[session event contract](docs/specs/session-events.md).
