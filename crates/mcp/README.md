# viamind-mcp

The Viamind agent harness: an [MCP](https://modelcontextprotocol.io) server, built on the
official Rust SDK [`rmcp`](https://crates.io/crates/rmcp), that exposes Viamind as tools.

```
MCP client ──transport──▶ ViamindServer (one per connection) ──▶ ToolRegistry ──▶ handlers
  clientInfo.name ─────▶ AgentId "name#conn" ──────────────────────────┘           │
                                                                                     ▼
                                     CommandHost (the engine): execute / undo / redo / snapshot
```

Built-in tools: `viamind_version` and `echo`. With a `CommandHost` registered, every engine
command is a tool too, plus `undo`, `redo` and `get_design`. The `viamind-mcp` binary has no
engine yet, so it serves the built-ins only.

## Use it from Claude Code

```sh
cargo build --release -p viamind-mcp
claude mcp add viamind -- "$PWD/target/release/viamind-mcp"
```

Any other MCP client works the same way: run `viamind-mcp` with no arguments and speak MCP on
its stdin/stdout.

## Expose engine commands

The engine implements `CommandHost`; `viamind-mcp` does not depend on its concrete types.

```rust
pub struct HostCommand { pub name: String, pub description: String, pub input_schema: serde_json::Value }

pub trait CommandHost: Send + Sync + 'static {
    fn commands(&self) -> Vec<HostCommand>;
    fn execute(&self, name: &str, input: Value, agent: &AgentId) -> Result<Value, String>;
    fn undo(&self, agent: &AgentId) -> Result<Value, String>;
    fn redo(&self, agent: &AgentId) -> Result<Value, String>;
    fn snapshot(&self) -> Value;
}

let mut registry = ToolRegistry::with_builtins();
register_host(&mut registry, engine)?; // engine: Arc<dyn CommandHost>
serve_stdio(registry).await?;
```

`register_host` adds one tool per command (same name, description and input schema) and the
built-ins `undo`, `redo` (the calling agent's own history) and `get_design` (`snapshot()`).
Registration is all or nothing: a duplicate or invalid name leaves the registry unchanged. Host
methods may block; they run on tokio's blocking pool. An `Err(message)` from the host comes
back to the agent as a tool result with `isError: true` and that message.

## Agent identity

Each connection gets a process-unique number when its `ViamindServer` is created. A tool call
runs as `AgentId("<clientInfo.name>#<number>")`, e.g. `claude-code#3`; the name comes from the
`initialize` handshake or the request's `_meta` client info, and is `unknown-agent` if absent.
Two sessions of the same client are therefore different agents. Handlers that need it register
with `ToolRegistry::register_with_context` and receive a `CallContext { agent }`.

## Transports

`serve(registry, transport)` runs one connection on any rmcp transport and returns the
`RunningServer` once the handshake is done; `serve_stdio(registry)` is the stdio shortcut.
`registry` is a `ToolRegistry` or an `Arc<ToolRegistry>`; pass clones of one `Arc` to serve
many connections against the same engine. For an in-process client (as the UI does in T-0026):

```rust
let (server_end, client_end) = tokio::io::duplex(64 * 1024);
let server = viamind_mcp::serve(registry.clone(), server_end).await?; // hand client_end to the client
```

`(Sink, Stream)` channel pairs and any `AsyncRead + AsyncWrite` work the same way.

## Add a plain tool

```rust
registry.register(
    "add",
    "Add two integers.",
    json!({ "type": "object", "properties": { "a": { "type": "integer" }, "b": { "type": "integer" } } }),
    |input| async move {
        let (Some(a), Some(b)) = (input["a"].as_i64(), input["b"].as_i64()) else {
            return Err(ToolError::InvalidInput("a and b must be integers".into()));
        };
        Ok(json!({ "sum": a + b }))
    },
)?;
```

Object results are returned as `structuredContent` plus a JSON text copy; a `ToolError` is
returned as a tool result with `isError: true` so the agent sees the message.

## Test

`cargo test -p viamind-mcp` runs the unit tests, `tests/stdio.rs` (spawns the binary and drives
raw JSON-RPC over stdio) and `tests/host.rs` (a fake in-memory `CommandHost` served over
`tokio::io::duplex` to real rmcp clients: tools/list, tools/call, undo/redo, get_design, host
errors and per-connection agent ids).

## Save a native project

`save_project {"path":"design.viamind.json"}` atomically writes a native project and
sets the active file path. Existing destinations are preserved unless `overwrite: true`
is provided. `save_project {}` saves the current active file, replacing it; it requires
an existing active path. `get_document_status {}` returns absolute `path` (or null) and
`dirty`. Saving preserves design undo/redo, and undoing back to the saved content clears
dirty state. Edits made while a save is in progress remain dirty. Both HTTP and stdio
support these tools. See [the saving contract](../../docs/specs/project-saving.md).

## Board outline

`set_board_outline` requires `outline`, containing `outer` and optional `cutouts`
rings of `{x_mm,y_mm}` vertices. Rings close implicitly; omit the repeated last
vertex. Supply explicit `outline: null` to remove the native outline. Geometry
validation rejects crossings, overlapping/nested cutouts and cutouts touching or
outside the outer boundary. Optional `expected` contains the `project_id` and the
exact current model `outline` from `get_design` (integer nanometres, or null).
A mismatch rejects without changing state. Changes are one undoable transaction;
outline-bearing native files use version 2. See [the outline contract](../../docs/specs/board-outline.md).

## Part properties

`update_part` accepts a part `id` and optional designator, value, footprint, x_mm,
y_mm, rotation_deg and side fields. Omitted fields remain unchanged. Supply
`expected` as the exact model part from `query_parts` or `get_design` to reject
stale edits. The expected snapshot uses integer nanometre coordinates, while new
coordinate fields are in millimetres. Validation is atomic and the entire edit
undoes as one transaction. `add_part` also accepts `expected_project` to guard a
placement draft against switching to a different document.
See [the editing contract](../../docs/specs/part-editing.md).

## New and Open

`new_project {"name":"Power board"}` creates an unnamed project; name defaults to
Untitled. `open_project {"path":"board.viamind.json"}` validates and loads a native
file. Both keep existing connections sharing the same document handle and reset
undo/redo and selection. Unsaved changes reject by default. To discard them,
provide `discard_dirty: true` and the current `expected_cursor` from
`get_session_state`. Stale cursors and edits during loading reject the operation.
Successful replacement starts a fresh event stream: waiting and reconnecting
observers receive an error requesting `get_session_state` resynchronization.
See [the lifecycle contract](../../docs/specs/document-lifecycle.md).

## Shared desktop selection

The live desktop engine also exposes `get_selection` and `set_selection`.
`get_selection` takes `{}` and returns `ids`, `revision`, and the last `origin`.
`set_selection` takes `{"ids":["<board part, track or via UUID>"],"mode":"replace"}`;
`mode` defaults to `replace`, and can also be `add`, `remove`, or `toggle`.
An empty replacement clears selection. Unknown IDs reject the entire request.
These session actions wake the desktop and preserve the document undo/redo stack.
Deleting a selected object or undoing its creation removes it from the selection.

## Verify a running desktop

The desktop serves streamable HTTP at `http://127.0.0.1:7878/mcp` by default.
Set `VIAMIND_MCP_PORT` when launching the desktop to use another port. Connect an
agent client to that URL; the standalone `viamind-mcp` binary above serves only
identification/echo tools, so use the desktop endpoint to edit the open design.

From the repository root, with the desktop running:

```sh
python3 scripts/mcp-smoke.py --url http://127.0.0.1:7878/mcp --report /tmp/viamind-mcp-smoke.json
```

This standard-library client performs the MCP handshake, discovers tools, and
uses two distinct editing agent sessions to place, move, select, delete, undo, and redo
temporary parts. It checks useful validation errors, exact nanometre positions,
selective undo isolation, and conflicts on overlapping edits. It returns a JSON
report and exits nonzero on failure. `--observe-seconds 15` pauses with the test
parts selected so you can see their appearance in the desktop.

Run while the design is otherwise idle: the test compares the full document
before and after. Cleanup deletes only the parts it created and restores the
previous selection if another user or agent has not changed it. The document
returns to its initial state; the edit audit history retains the test operations.
An observer also checks atomic snapshots, event attribution, reconnect cursors, wait timeouts,
and selection pruning order. The HTTP client handles JSON responses and SSE responses, including the empty
priming event and heartbeat frames permitted by the transport.

## Query board parts

`query_parts` reads the live board without editing it. Combine filters with AND:

```json
{
  "text": "10k",
  "side": "top",
  "bounds": {"min_x_mm": 0, "min_y_mm": 0, "max_x_mm": 50, "max_y_mm": 50},
  "limit": 100
}
```

Use `designator`, `value`, or `footprint` for exact case-sensitive matches, `text`
for a case-insensitive substring across those fields, and `ids` for known UUIDs.
Bounds include their edges and test the part's origin. The response contains
`parts` (full part records with integer nanometre positions), `total` (all
matches), and `next_after` (null when done). Results are in UUID order. For the
next page, repeat the same filters and pass `next_after` as `after`. `limit`
defaults to 100, maximum 1000. Restart pagination after design edits.
See [the complete query contract](../../docs/specs/part-queries.md).

## Scoped batch DRC

`run_drc` checks a captured project snapshot and returns `{source_cursor, report}`.
It leaves the model, selection, undo/redo and dirty state unchanged. Optional
`copper_clearance_mm` (default 0.2) and `edge_clearance_mm` (default 0.25) must be
finite 0..100 mm. `max_violations` defaults to 1000, limited to 1..10000. These
example defaults are not fabrication approval.

The scope covers persisted rectangular SMD pads, straight round-ended tracks and
annular through-via copper on the top/bottom layers. Checks use actual rotated
pad rectangles, track capsules and via copper with open drills removed. Opposite
copper layers and same assigned nets are exempt from electrical clearance;
either unassigned net is checked. Copper contact violates even a zero rule.

Additional optional rules (all finite 0..100 mm):

| Input | Default | Meaning |
| --- | --- | --- |
| `minimum_via_drill_mm` | 0.2 | Minimum via hole diameter |
| `minimum_annular_width_mm` | 0.1 | Minimum radial `(diameter-drill)/2` |
| `hole_clearance_mm` | 0.25 | Physical via-hole spacing, including same nets |
| `hole_to_copper_clearance_mm` | 0.2 | Via drill to foreign copper; own/same-net copper exempt |

Results carry tagged `pad` (part/pad ID, designator/number), `track` (track ID,
side/net) and `via` (via ID/net) references, measured/required Nm, actual pad/track/
via/hole coverage and source cursor. Through vias share both layers, with one
pair issue rather than duplicate layer issues. New violations distinguish
minimum drill/annular width, physical hole spacing and foreign hole/copper gap.

Read exclusions: legacy previews and unsupported extreme transforms include
stable part IDs/reasons (first 1000 details, exact total and truncation flag).
Absent outline excludes substrate checks. Curved/inner-layer tracks, unsupported
via types, pours, through-hole footprint pads, nonplated holes, drill-to-edge,
mask/silkscreen, net continuity and remaining fabrication checks are excluded.
Total copper shapes are limited to 100000; candidate-pair work to 1000000.
`truncated`/`incomplete_reason` mark limited results/work. Zero scoped issues is
not fabrication approval. Run again after edits and compare `source_cursor`.
Desktop **Board → Design rule check** uses these same typed rules, displays
coverage/stale state and selects persisted issue objects through shared selection.
See [pad foundation](../../docs/specs/batch-pad-drc.md) and
[track/via contract](../../docs/specs/track-via-drc.md).

## Watch design and selection changes

Call `get_session_state {}` for an atomic `project`, `selection` and `cursor` snapshot.
Then call `watch_events` with that cursor:

```json
{"after": {"stream_id": "<session UUID>", "sequence": 0}, "limit": 100, "timeout_ms": 30000}
```

The result contains ordered `events`, the next `cursor`, and `has_more`. Records have a
`sequence` and an `event` with `kind` (`design` or `selection`) and `data`. Design data is
the transaction change (including deltas and origin); selection data is the complete shared
selection. Apply events in order and retain the returned cursor. Drain remaining pages when
`has_more` is true, then wait again. Both stdio and HTTP support this tool.

`limit` accepts 1–1000 (default 100); `timeout_ms` accepts 0–30000 (default 0, immediate).
Without `after`, watching starts at the current tail. On timeout the page is empty and its
cursor is unchanged. Use one pending watch per observer; a cancelled call may occupy a worker
until its timeout. Cursors survive client reconnects to the same engine. They expire when
more than 1024 newer records arrive, or when the engine restarts. On a cursor error, call
`get_session_state` to resynchronize. Reading events preserves undo ownership and history.
See [the full session event contract](../../docs/specs/session-events.md).

### Electrical pads

`set_part_pads` takes `id`, an exact `expected` Part snapshot from `get_design` or
`query_parts`, and `pads`: rectangular SMD pads with `number`, `x_mm`, `y_mm`,
`width_mm`, `height_mm` and optional `net`. Omitted pad IDs preserve existing
identities by number; include an existing `id` when renumbering a pad. Empty pads
means no electrical pads. A stale snapshot, invalid size or duplicate number/ID
rejects the whole edit. Pads are included in part queries, events, undo and native
version-3 saves. Matching case-sensitive net names identify shared membership.

## Assembly CSV from a live design

`engine_registry` exposes `export_assembly_csv` in both desktop HTTP and headless
stdio sessions. For example:

```json
{"kind":"bom","path":"/home/me/design/bom.csv","overwrite":false}
```

`kind` is `bom` or `placement`. Include `expected_cursor` from `get_session_state`
to reject stale exports. The response contains `path`, `kind`, `component_count`
and `exported_cursor`. Each file is published atomically; existing files require
explicit overwrite and the active native path is protected. Exporting leaves the
native saved baseline, selection and design history unchanged. See
[generic assembly CSV conventions](../../docs/assembly-exports.md) before using
component-origin placement coordinates for assembly work.

### DXF outline import

`import_board_outline_dxf` accepts ASCII DXF `text`, required `expected` with the
current `project_id` and exact `outline` from `get_design`, optional `layer` (an
exact name), and `units`: `header` (default), `mm`, or `inches`. Header mode requires
supported nonzero `$INSUNITS`; explicit units intentionally override the drawing.
Closed straight LWPOLYLINE/POLYLINE and exact joined LINE loops are supported.
DXF +Y-up coordinates map to native -Y, preserving visual orientation.
Curves, bulges, 3D geometry, open/branching loops and invalid board topology fail
without changing the document. Import is one undoable outline change. The desktop
has the same operation under **Board → Board outline → Import ASCII DXF**. See
[the supported DXF subset](../../docs/specs/dxf-outline-import.md).

### Named nets and connection guides

`query_nets` returns case-sensitive name-sorted `nets` summaries with `terminals`
and `guide_count`. `after` is the preceding page's `next_after`; `limit` defaults
to 100 and is bounded to 1–1000. Only explicitly persisted, net-assigned pads
participate; built-in footprint previews invent no connectivity.

`query_connectivity` requires an exact `net` string. It returns `terminal_count`,
`guide_count`, transformed millimetre terminal centres and centre-to-centre
`guides`, plus `algorithm`. Both arrays use the same `offset` (default 0) and
`limit` (default 100, max 1000); continue at `next_offset` until null. Restart
pagination after a design change. Reads leave history, selection and events
unchanged. Small nets (up to 256 terminals) use exact `dense_prim`; larger nets
use a deterministic O(N logN) `coordinate_chain` approximation.

The desktop draws the same cyan guides and updates them after placement, pad/net
editing and undo. Guide count estimates routing work: no copper continuity
analysis exists yet, and touching same-net pads may already conduct. Guides may
cross or pass through obstacles; they are neither copper nor a DRC result.

## Straight tracks and through vias

The shared command catalog includes `add_track`, `delete_track`, `add_via` and
`delete_via`. For example:

```json
{"start_x_mm":10,"start_y_mm":10,"end_x_mm":20,"end_y_mm":10,"width_mm":0.25,"layer":"top","net":"GND"}
```

```json
{"x_mm":20,"y_mm":10,"diameter_mm":0.6,"drill_mm":0.3,"net":"GND"}
```

Creation returns a stable ID in `created`. Deletion takes that `id` and optional
exact `expected` snapshot; all four commands accept optional `expected_project`.
Coordinates use the document's Y-down frame. New copper is persistent, undoable
and replayable. Use `run_drc` for scoped clearance and drill checks; electrical
continuity and automatic routing remain separate work. Only straight top/bottom tracks and through vias are
supported; blind/buried/microvias and inner-layer tracks reject unsupported input.

### Grouped part placement

`transform_parts` requires `expected_project`, `expected` (exact full Part records
from `query_parts`/`get_design`) and `operation`. A changed or missing member rejects
the entire group. IDs and local pad geometry are preserved, and undo restores the
whole group in one step. The desktop uses this command for pointer release,
rotate/flip keys and Edit → Arrange part origins.

Operations are tagged with `kind`: `translate` plus integer nanometre
`delta: {x,y}`; `rotate` plus integer `degrees` in -360..360; `flip`; `align`
plus `axis: "x"|"y"` and `reference: "minimum"|"centre"|"maximum"`; or
`distribute` plus `axis`. Alignment and distribution use part origins, not body
edges. Distribution keeps outermost origins fixed. Rotation and flip act at each
part's own origin. Selection size is bounded to 10000 unique parts.
