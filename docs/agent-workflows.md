# Agent workflows

## Connect the full application

Build on Linux with `cargo build --release -p viamind`. From the repository root:

```sh
claude mcp add --transport stdio viamind-headless -- "$PWD/target/release/viamind" --headless
```

Every stdio process owns its own document; it does not share the desktop. For
the desktop's current document, launch `target/release/viamind`, then register:

```sh
claude mcp add --transport http viamind http://127.0.0.1:7878/mcp
```

These are user-run configuration commands following the public
[Claude Code MCP guide](https://code.claude.com/docs/en/mcp). Within Claude Code,
`/mcp` shows connection status. Use `VIAMIND_MCP_PORT` for another desktop port.
The separate `viamind-mcp` executable serves skeleton identification tools only.

The [complete generated reference](generated/mcp-tools.md) contains every
discovered definition, including full input schemas and nested definitions.
Tool annotations are metadata; each tool's schema and description define its
actual guards, units, limits and scope.

## Guard a component edit

An example request for an agent connected to an owned document:

> Read get_session_state. Place J1 at (10.125,15.25) mm and J2 at
> (30.125,15.25) mm, each with value "2-pin connector" and footprint name
> "EXAMPLE_2PIN". Guard placements with the current project ID. Read the exact
> J1 model snapshot, then use update_part with expected equal to that snapshot
> to move its origin to X=11.125 mm and change its value to "Input" in one
> transaction. Verify J2 is unchanged. Undo and redo your last transaction.

`expected.position` contains integer nanometres; new `x_mm`/`y_mm` fields use
millimetres. Copy the entire exact part snapshot rather than reconstructing it.
A rejected stale edit leaves the document unchanged. Refresh the snapshot and
reconsider the edit before retrying. Footprint names here create legacy graphical
previews; explicit electrical pads and net assignments are separate actions.

Undo/redo belongs to the calling connection. Another agent's history is separate;
overlapping changes can conflict. The desktop shares the same command engine.

## Run the verified example in isolation

```sh
python3 scripts/mcp-agent-workflow.py --binary target/release/viamind
```

This starts a fresh headless child, discovers required schemas, runs the original
placement example, asserts exact coordinates and guards, tests stale part and
full-cursor lifecycle rejection, then saves/reopens a NEW native file. Temporary
files are removed afterward. It never connects to your current desktop.

To retain the native design and verification report, supply a new destination
whose parent exists:

```sh
python3 scripts/mcp-agent-workflow.py --binary target/release/viamind --output ./agent-example
```

Existing files, directories and symlinks at that destination are refused. The
report records actual executable identity and checks; its project/part UUIDs
are freshly generated on each run. This example verifies application behavior,
not electrical completeness or manufacturing suitability.

## Save and replace documents deliberately

Use `save_project {"path":"/absolute/new-board.viamind.json"}` for a new file.
An existing supplied path is refused unless explicitly permitted by the saving
contract. `save_project {}` saves the current active file. Check
`get_document_status` for the active path and dirty flag.

Before New/Open, obtain the full cursor from `get_session_state`. If work is
dirty, save it or explicitly choose discard with `discard_dirty: true` and the
current `expected_cursor`. A cursor includes both stream ID and sequence;
copy both. Successful replacement resets history/selection and starts a fresh
event stream, so observers resynchronize before following events. Never discard
unsaved work merely to make an example succeed.

## Regenerate the reference

After changing registered tools, build the full application, then generate into
a NEW directory:

```sh
python3 scripts/generate-mcp-reference.py --binary target/debug/viamind --output ./mcp-reference-new
cp ./mcp-reference-new/mcp-tools.json ./mcp-reference-new/mcp-tools.md docs/generated/
python3 scripts/generate-mcp-reference.py --binary target/debug/viamind --check docs/generated
```

Review and commit both files. CI checks exact bytes against the freshly built
application and runs the isolated example. Host paths, timestamps and binary
hashes are excluded from the stable catalog; optional `--report ./new-report`
retains those verification hashes in a separate NEW directory. Both outputs
publish independently without replacing existing destinations.

Discovery follows all opaque pagination cursors, including empty strings, and
preserves complete tool objects. It has bounded deadlines, record/total byte
counts, stderr, notifications, pages and tools; oversized or invalid responses
fail instead of publishing a partial reference. See the
[public-source implementation contract](specs/generated-mcp-reference.md).
