# Automatic scoped DRC

## Sources and independent behavior

Public behavioral reference: [KiCad 9 PCB editor DRC documentation](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rules-checking) describes checking board constraints and reporting violations. Viamind implements its own existing scoped geometry checker, with explicit exclusions; this feature makes that checker refresh after edits rather than extending its fabrication coverage. No implementation code or proprietary assets were consulted.

Concurrency primitives follow the primary Rust documentation for [Weak](https://doc.rust-lang.org/std/sync/struct.Weak.html) and [Condvar](https://doc.rust-lang.org/std/sync/struct.Condvar.html). The worker observes a weak engine reference and exits when the runtime controller is dropped. A condition variable waits for work without polling the document.

## Runtime contract

- Desktop and MCP configure/read one shared automatic DRC runtime. Initially disabled; enabling checks the current document. Configuration is transient and does not affect project persistence, dirty state, selection, undo history or session event cursors.
- Every committed edit, undo, redo and project replacement invalidates the current result. Schematic-only design transactions conservatively rerun the board checker; schematic geometry and connectivity are not checked. Selection and saving alone do not. Configuration changes also invalidate results. A monotonically increasing generation identifies requests; a separate design revision and session stream protect publication against edits racing notification.
- One lazy worker debounces requests, keeps one coalesced pending request, captures a bounded immutable board snapshot under the engine lock, then checks outside that lock. A request arriving during checking supersedes the result; obsolete results cannot become current. Disable immediately clears the report. No unbounded queue or duplicate worker is created.
- Default debounce is 20 ms, configurable 0..2000 ms. Default snapshot limit is 5000 structural objects, configurable 1..10000; objects include parts, pads, tracks, vias, outline ring records and vertices. The checker receives the same project ID and board, with unrelated schematic/name data omitted. Board snapshot text is limited to 2 MiB before copying; designators, pad numbers and nets are at most 512 bytes each to bound repeated report labels. Over-limit snapshots produce an explicit skipped status without an all-clear report. Existing checker candidate-pair/report limits and coverage exclusions still apply.
- Status states are disabled, pending, running, ready, skipped or failed. Ready includes the checked snapshot cursor, generation, scoped report, snapshot capture duration, checking duration and request-to-publication duration measured with a monotonic clock. These are worker measurements, not desktop edit-to-visible latency. Pending/running never exposes an old result as current.
- The desktop polls lightweight status while the dialog is visible, requests repaint while enabled, and disables issue selection until the latest report is ready. Manual batch DRC remains available when automatic checking is disabled. Both configurations use the same validated rules.
- This is a bounded moderate-board foundation. Full manufacturing DRC, copper connectivity and a universal under-50-ms edit-to-visible guarantee are outside this increment. Parent performance acceptance requires actual native measurements.

## Verification

Deterministic barriers hold a worker during edits, configuration and project replacement; only the latest generation may publish. Verify configuration/status leave the design/history/cursor unchanged, selection does not rerun, invalid configuration is atomic, bounds are explicit, dropping the last controller releases the engine, and edits return while checking is blocked. HTTP clients and actual egui interaction exercise shared configuration and refresh. Record Linux build/test/fmt/clippy evidence and measured timing without converting measurements into an unsupported performance guarantee.
