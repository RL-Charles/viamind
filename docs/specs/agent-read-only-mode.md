# Startup read-only agent mode

## Public sources and intent

Original Viamind behavior, based on the public MCP
[tool discovery/call contract](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
and [tool annotations](https://modelcontextprotocol.io/specification/2025-06-18/schema#toolannotations).
Annotations describe behavior but are untrusted hints; admission is enforced by
trusted server code. No proprietary implementation or assets are used.

This increment adds process-wide startup `--agent-read-only` for the full
application's headless stdio and desktop HTTP endpoints. Default live apply
behavior and existing default tool definition bytes remain unchanged. Human
desktop editing continues through the same engine. Approval overlays, dynamic
switching, per-agent identity authentication and OS/filesystem isolation are
separate work. Reading supported local native/library/conversion input files is
allowed, so this mode is explicitly not a filesystem sandbox.

## Trusted admission

1. Each registry entry has internal effect classification, supplied only by
   trusted registration code. Existing general registration APIs and every
   unknown/new entry default to mutating. Pure observations require explicit
   trusted registration/classification. Client JSON, names, descriptions,
   annotations and `clientInfo` never authorize calls.
2. Restriction is retained on a registry clone and on subsequent registrations.
   Removing currently known writer entries alone is insufficient: later general
   registration must stay hidden and uncallable. There is no runtime API or
   client request to restore unrestricted admission on a restricted registry.
3. Restricted `tools/list`, definition lookup, visible count and direct dispatch
   agree. Writers are unavailable; direct calls to hidden names fail before
   deserialization, handler invocation, worker creation, engine mutation or I/O.
   Missing-tool errors use the existing MCP protocol behavior.
4. Pure observations include supported state/selection/status queries, bounded
   routing/cleanup plans, DRC/ERC, connectivity, native/library inspection and
   conversion/import previews. Explicitly audit every special host and external
   registration. `configure_auto_drc` changes session configuration and is
   unavailable; status queries remain available. Undo/redo, selection setters,
   edits, lifecycle replacement, native saves, exports, conversion publication,
   output-job saves/runs and detached library writers are unavailable.
5. The four existing board/schematic SVG/PNG tools mix in-memory rendering and
   optional file output. In restricted mode retain in-memory rendering, omit
   `path` from their advertised input properties, and explain the restriction.
   Reject **any supplied top-level `path`**, including null or malformed values,
   before invoking their handlers. An absent path reaches existing validation
   and rendering. Do not authorize other tools based on this field convention.
   Default live definitions/behavior are unchanged.
6. `load_footprint_definition` is an observation when available;
   `save_footprint_definition` is a writer even though it preserves the document.
   The same distinction applies to detached symbol/output-job settings.
7. Pure observations may allocate bounded snapshots, acquire existing admission
   permits and read existing files; they preserve the document, native bytes,
   active path, dirty flag, selection, history and engine event stream. Existing
   observer connection bookkeeping is not a document edit.

## Startup and visible state

- `viamind --headless --agent-read-only [--open PATH]` serves restricted stdio;
  it never binds a desktop port. All calls in this process share the mode.
- `viamind --agent-read-only [--open PATH]` serves restricted desktop HTTP and
  leaves human tools functional. Mode is set before any endpoint is exposed.
- Reject combinations with `--save-as`, `--no-mcp`, `--demo`, and every CLI
  export/output-job mode during argument validation, before file open, output,
  window creation or listener startup. Duplicate/missing/unknown options retain
  the existing parser rules. Help documents the new flag and scope.
- Headless diagnostics and MCP initialization instructions explain that agents
  can observe but cannot edit or write files. Existing live instructions remain
  unchanged by default. Desktop status visibly says **Agents read only** even
  in a compact viewport; failed endpoint startup stays distinguishable.
- Preserve existing public desktop launch helpers through default live wrappers;
  add an explicit mode-aware entry point if necessary. No tool toggles the mode.

## Verification before merge

1. Independent review of this public spec before implementation.
2. Registry tests use counter handlers to prove hidden direct calls never run,
   including malformed arguments, cloned restricted registries and subsequently
   registered default mutators. Pure observations remain callable, definitions
   and visible counts agree, and PNG response classification is preserved.
3. Mixed rendering tests prove restricted schemas omit `path`, every supplied
   path fails before a handler runs, and absent-path observations succeed.
   Default live schemas and calls remain exact. Classification is explicit,
   never inferred from a tool name or client hints.
4. Real stdio and two-client HTTP tests discover only admitted tools; attempt
   model/history/selection/native/export/library/job changes and all four render
   file outputs. Compare complete session snapshots, protected native bytes and
   destination absence. Retain clean source/executable identity. Verify valid
   in-memory render, query and DRC behavior using original explicit geometry.
5. A shared engine with restricted clients and a trusted human/live peer proves
   legitimate human mutations remain undoable and visible to both observers.
   Restricted calls cannot change mode, even with forged client identity/hints.
6. Actual compact UI input tests and a readable native WGPU framebuffer verify
   mode status and human editing. Retain source/binary/capture hashes and honest
   native inspection limits; inspection hosts close after fsynced proof.
7. Formatting, strict all-target workspace Clippy, workspace tests/build,
   canonical default full-tool reference check, isolated workflow, independent
   implementation review and combined exact-head Linux CI pass before merge.

S-027's approval ghost overlay remains open after this bounded read-only subset.
