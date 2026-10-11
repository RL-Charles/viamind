# Headless design sessions

## Public sources

- [MCP stdio transport](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#stdio): newline-delimited JSON-RPC on stdin/stdout, diagnostics on stderr, client-spawned server lifecycle.
- Viamind's CLI and file options below are original product decisions.

## Behavior

- `viamind --headless` serves the complete live engine toolset over stdio. It creates no window and needs no display. The session ends when the client closes stdin.
- `--open PATH` loads an existing native Viamind project before serving. Invalid or missing files fail clearly before the session starts. It works with desktop mode too.
- Without `--open`, create a fresh Untitled project. `--save-as PATH` saves the final document through the native format after a clean headless session end. The input file is unchanged unless the output path explicitly names it. Save errors exit nonzero.
- Read, edit, selection and per-agent undo/redo tools behave exactly as in the desktop. A headless stdio connection is one agent session. Shared desktop TCP transport remains available in desktop mode only.
- `--help` prints usage and exits, `--version` prints the build version. Unknown or duplicate options and missing paths are errors. `--demo` is exclusive of design options. `--headless --no-mcp` is invalid. `--save-as` requires `--headless`.
- `VIAMIND_MCP_PORT` applies only to the desktop HTTP endpoint. Malformed values in desktop MCP mode are errors; a headless session does not bind any port.
- Session diagnostics go to stderr. Stdout in headless mode contains only protocol messages.

## Acceptance

A spawned client with display variables removed loads a project, discovers all design tools,
edits, queries, undoes/redoes, closes stdin and reads a valid saved native project. The input
file remains byte-identical. Invalid CLI options and input/output errors return nonzero.
