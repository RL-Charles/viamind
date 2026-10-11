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

Download an optimized tarball from the **Linux bundle** workflow's artifacts, or
build one with `python3 scripts/package-linux.py /tmp/viamind-linux-build`.
See [Linux bundle instructions and runtime requirements](docs/linux-bundle.md).

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

## Check copper and via drill rules

Open **Board → Design rule check** for read-only batch checks of rectangular SMD
pads, round-ended straight tracks and through-via annular copper. Choose copper/
board-edge clearance, minimum via drill and radial annular width, drilled-hole
spacing and foreign copper-to-hole clearance. Results include stable object IDs,
measured distances and actual coverage; select an issue to highlight its objects.
Legacy previews need explicit pads. Pours, unsupported copper types, connectivity
and additional fabrication checks remain excluded; zero scoped issues is not
fabrication approval. Agents use the same `run_drc` rules and snapshot report.
Reports become stale after design edits.

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

Drag an already selected part to move the selection with an amber local preview;
release commits one guarded edit, and Escape or Space cancels. On the canvas,
**R** rotates selected parts 90° and **F** flips their mounting sides. The Edit
menu aligns/distributes **part origins**; agents use the same `transform_parts`
command with exact expected Part snapshots to protect grouped edits.

Selected parts expose **Edit electrical pads…** for persistent rectangular SMD
pads, local dimensions and named net assignments. Apply and Reload protect drafts
against concurrent agent changes. These pads rotate and mirror with their parent
part; absent explicit pads retain the built-in graphical preview. Explicit pads
use native schema version 3. Cyan connection guides follow explicit net-assigned
pads through placement and undo; `query_nets` and `query_connectivity` expose the
same connectivity to agents. Guides estimate routing work without copper
continuity analysis and are not copper or a DRC result. Routing, other pad shapes and schematic net transfer
remain roadmap work.

## Export assembly tables

Produce generic BOM and component-origin placement CSV without opening a window:

```sh
viamind --export-csv bom --open design.viamind.json --output bom.csv
viamind --export-csv placement --open design.viamind.json --output placement.csv
```

Existing destinations require `--overwrite`. Agents export unsaved shared designs
with `export_assembly_csv`; exports preserve native dirty state and undo history.
See [assembly export conventions](docs/assembly-exports.md) for exact units,
bottom-side transforms, grouping, file publication and manufacturer limitations.

## Straight copper and through vias

Use **Board → Copper tracks and vias** for coordinate-based placement and deletion.
Track endpoints/width and via position/diameter/drill are in millimetres; optional
net labels are shared with agents. Supported tracks are straight segments on top
or bottom copper. Supported vias are circular through vias; their drill must be
smaller than their outer diameter. Undo restores exact geometry and identity.
Agents use `add_track`, `delete_track`, `add_via` and `delete_via`; optional expected
project/object snapshots guard stale edits. Native files containing copper require
schema version 4. Older outline/pad-only files keep their existing schema version.
Copper continuity, interactive routing, curved tracks, inner layers and other via
types remain subsequent work. Pad-only connection guides do not yet account for
tracks or vias. See [the copper contract](docs/specs/copper-tracks-vias.md).

## Footprint libraries

The electrical pads editor can export applied rectangular SMD geometry to a new
native library file and reuse it on another component. Matching pad numbers keep
their identities and nets; placement stays unchanged and import is one undoable
edit. Agents share `apply_footprint`, `import_footprint` and `export_footprint`.
See [footprint library instructions](docs/footprint-libraries.md) for the strict
integer geometry format and bounds.

## Scoped outer-layer fabrication export

```sh
viamind --export-gerber --open board.viamind.json --output new-fabrication-directory --mask-expansion-mm 0.05
```

The output includes aligned top/bottom Gerber X2 copper and mask openings,
polygon profile/cutouts, XNC (documented Excellon subset) plated through-via
finished-hole drill and a manifest. All files use mm, the model origin and
X-right/Y-up top view; bottom files are not mirrored. Every part needs explicit
pads and the board needs an explicit outline. Existing destinations always
reject. Mask expansion defaults to0mm; vias are untented. **Board → Export
fabrication** and MCP `export_fabrication` export the same immutable snapshot.

Only explicit rectangular SMD pads, straight outer tracks and through vias are
supported. Legacy footprint previews, inner layers, pours, paste/silkscreen,
component through holes/slots and X3 assembly/material instructions are excluded.
This is a scoped geometry export, not fabrication certification. See
[the specification](docs/specs/fabrication-gerber.md) and the independent reader
check in [scripts/verify-fabrication.py](scripts/verify-fabrication.py).

## Board inspection SVG and PNG

```sh
viamind --export-svg --open board.viamind.json --output new-board.svg --view-layers top_copper,outline
```

MCP `render_board_view` returns the same bounded, immutable SVG snapshot with its
source cursor, selected layers and region metadata. Explicit pads, round-cap
tracks, drilled through vias and polygon cutouts retain model X-right/Y-down
top-view geometry. Existing output paths are preserved. Legacy previews and
unsupported geometry are reported. `--export-png` uses the same view options for
transparent RGBA PNG; MCP `render_board_png` returns actual image content with
source metadata. PNG is bounded to 4194304 pixels and 8 MiB. See
[board view instructions](docs/board-views.md) for cropping, viewport options,
agent requests and independent SVG verification.

### Schematic capture

Switch between **PCB** and **Schematic** in the desktop toolbar. In schematic mode,
create a sheet, add symbols and numbered pins, then add wires, junctions and
local/global/power labels. Select an object in **Schematic objects** or click its
anchor to edit it. Apply uses the same guarded, undoable sheet command as MCP;
a concurrent agent edit requires explicit reload. Middle drag or Space+drag pans,
wheel zooms, and Home fits the sheet. Schematic-only documents open in this mode.

The original regulator example is
[`fixtures/schematic/regulator.viamind.json`](fixtures/schematic/regulator.viamind.json).
`query_schematic_connectivity` exposes bounded exact native connectivity over MCP,
including transformed pin endpoints and local/global/power label scope. The
regulator fixture yields VIN (2 pins), 3V3 (2 pins) and GND (3 pins). Ambiguous
crossing labels or exceeded work limits return errors without partial nets.
`run_erc` adds read-only native pin checks over that exact graph, with a configurable
66-pair compatibility matrix and stable actionable pin/net identities. Reports
state supported coverage and any work/result truncation; they never certify full
ERC. The regulator reports missing power-output sources on VIN and GND because
power labels name nets without supplying power. Desktop ERC results, hierarchy,
buses and board annotation remain separate work. See the
[connectivity behavior specification](docs/specs/schematic-connectivity.md) and
[native ERC policy](docs/specs/schematic-erc.md).
