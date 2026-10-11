# Bounded resumable automatic DRC status events

Ticket: T-0109; story S-024. Independent Viamind behavior, written before implementation.

## Public sources

- [MCP2025-03-26 tools](https://modelcontextprotocol.io/specification/2025-03-26/server/tools): named observation tools with strict input schemas and tool execution errors.
- [MCP2025-03-26 resources](https://modelcontextprotocol.io/specification/2025-03-26/server/resources): resource notification subscriptions are a distinct optional protocol capability. These tools do not claim that capability.
- [KiCad9 PCB DRC](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rule-checking): distinguish violations and checker coverage from general document edits. No code/assets or proprietary formats are used.
- [Rust Condvar](https://doc.rust-lang.org/std/sync/struct.Condvar.html): predicates checked under the same mutex, bounded waits and spurious wakeups.

## Existing contract

Automatic DRC is transient, disabled by default, generation guarded, coalesced and bounded. The worker releases the engine while checking, rejects stale results, and exposes current configuration/phase/source cursor/report/coverage through existing status observation. Design and selection already have a separate1024-record session journal. Publishing DRC must preserve that journal, document/selection/history/native bytes, dirty state, and its exact edit guards. Existing automatic DRC configuration, status responses, checker geometry/rules and UI stay compatible.

## Two additional observation tools

`get_auto_drc_event_state {}` returns `{status,cursor}` captured atomically under the automatic DRC state lock. `status` is the existing complete current status, including a report only when ready. `cursor` has strict `{stream_id,sequence}`, is independent from design event cursors, starts at sequence0, and belongs to this controller lifetime. It survives document New/Open while generations invalidate; restarting the engine creates a different stream. No report array is retained in event history.

`watch_auto_drc_events {after?,limit?,timeout_ms?}` returns `{events,cursor,has_more}`. Unknown fields reject. `after` omitted/null starts at current tail; explicit cursors are reusable across initialized connections and reconnects. Limit defaults100 and accepts1..128; timeout defaults0 and accepts0..30000ms. Validate all inputs and cursor before waiting. Wait with the state condvar while releasing its mutex, without an engine/design/native lock. Spurious wakes retry until records or the monotonic deadline. If records exist, return immediately. Timeout returns an empty page with the requested cursor. Each page contains strictly increasing sequence numbers after the request, and cursor advances only to the last returned event; has_more identifies another retained page.

Different controller stream, future sequence, and evicted history reject explicitly with guidance to call `get_auto_drc_event_state` for a new atomic baseline. The event capacity is128; the oldest record is evicted before adding record129. Exact boundary: one event immediately before the oldest retained sequence is a valid resume cursor. Polling never renews/modifies state and never changes generation.

## Journal records and transitions

Every record contains sequence plus a small summary of generation, phase, optional source_cursor, optional report summary, and optional message. Report summary copies scalar violation count, truncation, incomplete-reason presence, excluded-part count and explicit-outline-checked status from the accepted existing report; consumers use an atomic current state to obtain detailed violations and match generation. It does not assert successful manufacturing or complete global DRC coverage. Message is truncated at a UTF-8 boundary to512bytes; other event fields are fixed size. No project, source strings, rule configuration, violation array or entire report is copied into event history.

Record each accepted configuration change (Pending or Disabled), enabled design invalidation (Pending), worker start (Running), and accepted terminal publication (Ready, Skipped or Failed). Configuration validation failure emits nothing. A superseded worker never publishes a terminal event or report. Coalesced generations may have Pending without Running/terminal. Running may be superseded before snapshot capture; absence of a terminal event then reflects supersession. Disabled ordinary edits emit no DRC events. Selection alone emits none. Current source_cursor is present only when existing status contains it; Pending/Running events have none. On accepted terminal publication it identifies the actual captured design state. New/Open invalidates the generation, and no old-stream result can publish. Ordinary selection edits during checking do not invalidate geometry; the accepted source cursor remains the original capture cursor.

All mutations follow existing engine→automatic-state lock order. Watch/state reads use only the automatic-state mutex. No state→engine lock acquisition is added. Every journal append wakes observers. Existing worker drop wakes and stops its worker; observers hold their handle for the duration of a bounded call. No new worker/thread per journal and no channel with unbounded subscriber queues.

## Admission and parity

Both tools are audited observations and available in process-wide startup read-only mode; configure remains a writer and excluded. Actual UI and MCP changes already drive the same automatic controller, so no new UI action is introduced. Existing `watch_events`, session schemas, automatic status schemas, project/native versions, undo and save behaviors stay unchanged. Catalog definitions are generated from the actual full application after implementation.

## Acceptance and evidence

1. Independent public-source review approves this spec before Rust edits.
2. Deterministic tests cover atomic baseline, exact128 retention/page boundaries, wrong/future/expired/design cursor errors, strict bounds, selection/no-disabled-edit invariants, timeout and long-poll wake. Existing injected blocked checker covers supersession, config/disable, document replacement and checker failure recovery; extend it with exact event assertions rather than timing-dependent scheduling.
3. Actual initialized two-client HTTP tests independently resume from one baseline, observe configure/run/terminal, perform peer edit/undo and same-UUID reopen, verify status generation/source, and preserve full document/history/status/native invariants for observations. Read-only HTTP/stdio list both observers and reject configuration. Actual retained production stdio/desktop probes use owned scratch documents only.
4. Four Linux gates, strict all-target workspace lint, actual catalog/exact drift/workflow, independent source/evidence review and fresh combined exact-head CI pass. Historical product proof and final integration sources are labeled separately. No native cadence/OS-input claim follows from backend observation checks.

## Scope

Only automatic scoped DRC transitions are journaled. Explicit batch DRC, ERC, manufacturing job progress, OS scripts, approval hooks, native protocol resource subscriptions, persistent event history, and automatic report history remain separate contracts. The existing design/selection event stream and this DRC stream are intentionally independently ordered; there is no cross-stream total-order claim.
