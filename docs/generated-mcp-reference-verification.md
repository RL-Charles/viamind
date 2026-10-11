# Generated MCP reference verification

## Source and Linux gates

The retained [master report](verification/t0096/master.json) identifies tooling
source `36245f33d66be50d916d422156ba58a3e97ff537`, based on main
`9a80afc944da0531ecf9c469775ff48bafd972d9`. The queued build source was
`fb1cfcfecc4ef3b62e4c2b5a58ca71361f6d1408`; the later commit only corrected
Python tooling. Their complete `crates`, `apps`, `Cargo.toml` and `Cargo.lock`
inputs are identical. Rust inputs stayed frozen while queued and building.
The corrected Python tooling ran after the fresh application build.

Owned gate 40063 completed successfully: formatting, strict workspace Clippy
including all targets, **664 workspace tests passed, zero failed, one ignored**,
and a workspace build. All **13 adversarial Python tests** passed.
The [complete log](verification/t0096/full-linux.log),
[commands](verification/t0096/full-linux.sh) and
[queued source record](verification/t0096/queued-build-source.json) are retained.
This is a documentation/tooling change with no native rendering changes.

## Actual application discovery and workflow

The unique full application is retained at
`~/.cache/viamind/native-artifacts/t0096/viamind`, SHA-256
`bc2db97ec25fb88ae22a122ec79bb2345f6ecd1748d84ecdab6155cac85305dc`.
Each probe started its own fresh headless child and temporary working directory.

- [Discovery](verification/t0096/discovery.json) found **81 tools** using MCP
  `2025-03-26`. The committed [JSON](generated/mcp-tools.json) and
  [Markdown](generated/mcp-tools.md) retain complete discovered definitions.
- The [original workflow](verification/t0096/workflow.json) passed **18 checks**:
  required schema discovery, exact Nm placements and guarded edit, untouched
  second component, stale part rejection, undo/redo, NEW native save, stale full
  cursor rejection, collision preservation and exact native reopen with a fresh
  stream. The [resulting native design](verification/t0096/agent-board.viamind.json)
  is retained and hashed. The runtime temporary active path was removed afterward.
- A second discovery matched the committed reference exactly. Deliberate drift
  and an existing publication destination were rejected while preserving every
  destination byte; [actual CLI results](verification/t0096/reference-checks.json)
  include their expected nonzero exits.

The master report hashes all nine retained artifacts, including both catalog
files. Binary identity was checked before and after discovery and workflow.
Historical AppImage preflight results are separate from this fresh combined
proof and are not used as its source identity.

## Scope

The workflow uses legacy graphical component footprints. It establishes MCP
behavior and native document preservation; it does not establish electrical
completeness, manufacturing suitability, hosted documentation, or GUI rendering
performance. The [user guide](agent-workflows.md) explains desktop versus stdio
ownership and regeneration. Exact-head combined CI and independent review are
recorded on the ticket and pull request after this evidence commit.
