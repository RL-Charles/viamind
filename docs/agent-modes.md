# Agent modes

The desktop status bar shows the current agent mode. Select **Agent modes…** to choose **Live apply**, **Approval**, or **Read only** for every MCP connection attached to this application.

- **Live apply** applies agent edits immediately and is the default.
- **Approval** lets agents prepare bounded private PCB proposals. Open **Board → Agent branches**, inspect the green/red geometry, then **Accept proposal** or **Reject proposal**. Acceptance is one undoable edit. A stale proposal requires a new preparation; peer edits are preserved. Schematic edits, library changes, native saves and exports are unavailable to agents in this mode.
- **Read only** permits audited observations and in-memory views. Human editing remains available.

A mode change can refuse while an admitted agent operation is unfinished. Retry after it completes. Changing modes neither accepts a proposal nor extends its lease. Initialized clients receive a tool-list change notification; `get_agent_mode` reports the effective current restriction.

Start desktop HTTP or headless stdio with `--agent-approval` to begin in Approval. A headless process can prepare proposals but has no human desktop acceptance control. Start with `--agent-read-only` for an immutable restriction that the selector cannot lift; restart without that flag to allow agent edits. The two flags are mutually exclusive and reject incompatible save/demo/no-MCP/CLI export options before input or output is opened.

These controls govern this application's MCP tools. They do not authenticate clients or restrict trusted EngineHandle callers or other processes.
