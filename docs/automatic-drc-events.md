# Resumable automatic DRC observations

`get_auto_drc_event_state {}` returns current complete automatic DRC status and an atomic event cursor. Start or resynchronize here, then call `watch_auto_drc_events` with that cursor as `after`. Save the returned cursor and reuse it after reconnecting. Each observer keeps its own cursor; reading never consumes another observer's events.

```json
{"after":{"stream_id":"<DRC-stream-UUID>","sequence":0},"limit":100,"timeout_ms":1000}
```

The DRC stream is independent of the design/selection stream from `get_session_state` and `watch_events`. DRC completion does not stale document edit guards or change project, history, selection, dirty state or native bytes. New/Open with automatic DRC enabled invalidates its generation while retaining this controller's DRC stream; restarting creates a new stream. Configuration remains disabled by default and `configure_auto_drc` remains a writer. Both observations are admitted at startup in agent read-only mode.

The128-record history contains small summaries of Pending, Running, Ready, Skipped, Failed and Disabled transitions. A summary has generation/phase/source cursor, at most512 UTF-8 message bytes and optional exact scalar report facts: violation count, truncation, incomplete-reason presence, excluded-part count and explicit outline coverage. Detailed violation arrays exist only in current ready status; match generation when reading them. Scoped facts do not establish full manufacturing coverage or release acceptance.

Limits are1..128 events and0..30000ms timeout, default100/0. Omitted/null `after` starts at the current tail. Wrong, future or expired cursors explicitly require a new atomic baseline. A page advances only to its last returned event and `has_more` identifies another page. Bounded long polls release the automatic state mutex and take no document/native lock. One outstanding poll per observer is recommended. This is application tool polling; no MCP resource-notification subscription capability is added.

Coalescing can leave Pending generations without Running/terminal records. A Running generation superseded by an edit, reconfiguration or enabled document replacement cannot publish a terminal result. Selection and saves do not rerun geometry checks; the source cursor remains the original snapshot capture even if later selection advances the design stream. Invalid configuration emits nothing. Valid configuration followed by worker spawn failure records Pending then Failed and wakes observers even though configuration returns its existing error.

## Verification

[Behavior spec](specs/automatic-drc-events.md) received independent primary-source approval before implementation. [Master provenance](verification/t0109/master.json) distinguishes historical production source `b83293fb964fa4ca7f94c2fce6794e04009e1cd0` on `c58193c` from final combined integration.

The [full Linux gate](verification/t0109/linux-full.log) passed756 workspace tests/0failed/1ignored, formatting, strict workspace/all-target Clippy and workspace build. Actual90-tool discovery/exact drift,18 isolated workflows and13 tooling tests pass. The initial [focused gate](verification/t0109/focused.log) passed20 checks; the full gate additionally includes blocked-selection source-cursor, lifecycle and terminal-summary assertions. Deterministic injected blocked workers prove stale-generation rejection, edit liveness, selection capture, disable/reconfiguration, New and failure recovery. Actual initialized HTTP tests cover reconnect/page/long-poll, peer edit/undo, same-UUID native reopen, read-only admission and exact128 retention bounds.

The uniquely retained executable SHA256 `5a160c98758db56dd8db9918d5f6f9d6f3fbe79254e8ff43865b24b787862bca`, actual desktop PID2047665/HTTP7939, passed [27 two-client HTTP checks](verification/t0109/http-proof.json). It separately passed [27 actual headless stdio checks](verification/t0109/stdio-proof.json). Both portable probe runs include fresh read-only stdio admission/refusal and protect actual native bytes. Production checks establish normal transport semantics; deterministic race tests have separate scheduling provenance. The [portable probe](verification/t0109/production-probe.py) requires `--binary`, `--repo`, NEW `--directory`, and optional owned `--url`; it replaces only an explicitly owned scratch engine and retains its native file in the output directory.

Cua launched the exact retained executable. Its screenshot was viewed and shows the owned empty PCB document; degraded accessibility metadata is only discovery evidence. No new desktop action, OS-input, checkbox state or native cadence claim is made. Explicit batch DRC, ERC, manufacturing progress, executable hooks and persistent full report history remain separate contracts. Independent final combined verification and exact-head CI remain merge requirements.
