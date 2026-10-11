# Bounded session-owned PCB agent presence (T-0103)

## Public behavior and independent scope

Parent S-026 requests colored agent cursors, selections and working labels.
The public [Microsoft Live Share coediting documentation](https://learn.microsoft.com/en-us/visualstudio/liveshare/use/coedit-follow-focus-visual-studio-code)
describes visible collaborator cursors, selections and participant flags. Accessed
2026-10-11 for behavior only; no implementation or assets are copied.
Viamind implements its own explicit PCB presence protocol. This increment covers
the PCB canvas; schematic presence, camera following and collaborative document
branches remain separate work. Presence communicates attention, never ownership
or a lock on copper or components.

## Session identity and protocol

Add `set_agent_presence`, `clear_agent_presence` and `query_agent_presence`.
The first two use the server's trusted process-unique connection number, carried
in internal CallContext separately from clientInfo.name. Input cannot choose an
owner, connection number or another participant's display name. A direct registry
writer call without a trusted connection refuses. Existing agent attribution and
per-connection undo remain unchanged.

Set input is a strict object with required `expected_project` UUID,
`expected_stream` UUID, `cursor` integer-Nm point, `selection` board-object UUID
array and `working_on` text. Cursor components are bounded to +/-10000 mm.
Selection has at most 64 distinct existing board part/track/via IDs; duplicated,
missing, deleted and schematic IDs reject the entire update. Empty selection and
empty working text are valid. Working text is trimmed, control-free, at most 80
characters and 256 UTF-8 bytes. Borrowed array count and text/path-independent
scalar bounds precede typed secondary allocation. Unknown fields reject.

Clear input contains only required expected project/stream guards. It removes
only this connection's entry, and absent own presence is an idempotent success.
Set and clear reject after New/Open/reopen, including reopening the same project
UUID with a different stream. Ordinary edits, selection events, undo and redo do
not invalidate a matching stream. No sequence guard is required for attention.

Query takes only an empty object. It returns current project UUID/full event
cursor and at most 32 visible entries, ordered by connection number. Each entry
contains opaque numeric connection ID, sanitized bounded display label, cursor,
selected IDs, working text, stable palette index and remaining lease milliseconds.
Client names are presentation only, truncated at character boundaries to 48
characters/192 bytes after removing controls; blank names use `Agent`. The server
connection number remains visible alongside the label to distinguish identical
names. Lease milliseconds are transient and excluded from document semantics.

## Bounded lifetime and preservation

Each successful set renews a fixed 30-second monotonic lease. Query and rendering
never renew it. Expired entries are absent at the exact deadline; reconnecting
creates a new identity. Disconnect may leave the prior marker until expiry;
immediate transport-disconnect cleanup is not claimed. At most 32 active leases
exist. A new participant at capacity refuses without evicting others; an existing
participant can update or clear. Prune expired entries before admitting new ones.

Store presence in a separate bounded mutex shared by EngineHandle clones, outside
Project/Engine history and persistence. Use a consistent engine-before-presence
lock order. New/Open stream changes discard old presence before it can be returned
or painted. Deleted selected objects are omitted from query/render snapshots;
the remaining marker and working label survive until renewal or expiry. Updates,
clear, expiry and queries preserve the full SessionSnapshot, event cursor, human
selection, undo/redo, native bytes, active path and dirty state. A failed update
preserves the prior lease and content. No background worker or new model version.

In startup read-only mode query is a trusted observation; set/clear are default
blocked writers, even though they do not alter the design. Mode does not change
per connection. Live mode retains existing tool behavior and adds these tools.

## Native PCB overlay

Paint a small colored synthetic arrow/crosshair at each in-viewport agent cursor,
its sanitized `name #connection` and `working_on` label, and colored outlines of
selected visible board objects. Stable palette assignment depends on trusted
connection identity; colors may repeat and labels disambiguate. Use the existing
board camera transform, visible layers and clipped canvas painter. Bound rendering
to the 32 leases and 64 selections each; avoid cloning or scanning the full board.
Selected parts, tracks and vias use their cached visible selection bounds as
colored axis-aligned rectangles. Missing/hidden/offscreen geometry is skipped.

The overlay accepts no pointer/keyboard focus, changes no camera or human
selection, and paints above copper. Clip labels within the PCB canvas at 640x400
and normal size. Successful updates wake an idle native UI through callbacks
after releasing both locks, without appending a design or selection event.
While markers exist, request bounded repaint at most every
250 ms so expiry disappears without other document edits. No frame-rate claim.

## Required verification

- Deterministic exact-deadline lease tests using an injected monotonic instant;
  32/33 capacity, renewal, rejected update preservation and per-session clear.
- Real two HTTP clients with identical/forged names remain separate; writers
  cannot spoof owner or clear a peer. Fresh stdio and read-only discovery/direct
  refusal cover the same trusted connection boundary.
- Max/max+1 borrowed counts/text, integer coordinate bounds, duplicate/unknown
  IDs, lifecycle/reopen stream guards and deleted selections preserve design and
  files. Actual human edit/undo still works while presence is displayed.
- Actual compact UI shape/label/clip checks, and readable self-closing native WGPU
  image of two agents with cursor/selection/working labels. Preserve full session
  and native bytes on every native UI pass; state local inputs and cadence limits.
- Linux formatting, strict workspace/all-target Clippy, meaningful focused and
  workspace tests/build, actual regenerated catalog/drift check, independent
  implementation review and fresh combined exact-head CI before merge.

Story S-026 remains scoped accurately: this ticket proves explicit leased PCB
presence; automatic agent activity inference and schematic presence are absent.
