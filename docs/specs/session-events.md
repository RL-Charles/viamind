# Resumable session events

Public references: MCP [tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
define JSON inputs and structured results; [transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
define stdio and Streamable HTTP. The following cursor and long polling contract is Viamind's own
tool API. It works on both transports and does not require unsolicited notifications.

## Contract

- `get_session_state {}` atomically returns `project`, `selection`, and `cursor`.
  Start here to initialize a local view, or recover after a cursor error.
- `watch_events {after?, limit?, timeout_ms?}` returns `events`, `cursor`, and `has_more`.
  Each record has a sequence and a tagged `event`: `design` contains the existing transaction
  change event; `selection` contains the complete selection snapshot. Origins are preserved.
- Cursors contain a random session `stream_id` and a monotonically increasing `sequence`.
  They identify the last consumed record, independently of transaction IDs or agent connections.
  Pass the returned cursor as `after` on the next call, including after reconnecting.
- Omitted/null `after` starts at the current tail. `limit` defaults to 100, accepts 1–1000.
  `timeout_ms` defaults to 0 (immediate), accepts 0–30000. Existing records return immediately;
  otherwise wait until a change or timeout. Timeout returns an empty page and the same cursor.
  `has_more` means another page was already available when this page was read.
- The engine retains the last 1024 records in memory. An expired, future or foreign-session cursor
  produces an actionable tool error: refresh via `get_session_state`. Restarting the engine creates
  a new stream, even when loading the same project. Cursors are not saved in the document.
- Snapshot and cursor are read under the same engine lock. A change committed during snapshot
  delivery is therefore available after its cursor. Waits release that lock; the condition variable
  uses the same mutex to prevent lost wakeups. Multiple observers consume independent cursors.
- Applied, undone and redone transactions each emit one design record. Deleting selected parts emits
  design first, then the pruned selection. Selection updates emit only when IDs change. Failed edits,
  no-ops, queries and waits emit nothing. Watching changes neither undo/redo nor persistent event logs.

## Client use and limits

Read snapshot → watch after its cursor → apply records in order → retain returned cursor.
Drain pages while `has_more`, then wait again. A new client connection can continue with the same
cursor; this does not transfer the previous connection's undo ownership. The history is bounded by
record count, and a large transaction may contain many deltas. Long polls occupy a blocking worker
for at most 30 seconds; clients should keep one outstanding watch per observer. Cancellation or
disconnect can leave that worker waiting until the timeout. No DRC events are advertised until the
application has a DRC model. Existing document subscribers and repaint callbacks retain their API.
