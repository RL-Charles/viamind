# Agent admission handler inventory

T-0117 pre-implementation inventory, source stack T113 `842e9c1` atop T112 `b761793`. This table describes original handler completion boundaries to preserve through the new retained admission executor. Final multi-unit/package registrations require another additive audit before combined verification.

| Registration | Trusted effects | Completion boundary |
| --- | --- | --- |
| `builtin` | Explicit version/capability observations; general registration defaults to writer | Synchronous bounded handler result |
| `host::register_host` / `EngineHost` | Only `EngineHost::is_observation` whitelist is observation; original engine commands, undo/redo, selection, lifecycle, auto-DRC configuration and native saves are writers | `host::blocking` awaits `spawn_blocking`; synchronous EngineHost dispatch and its native/library/import publication finish before closure returns |
| `assembly::register` | `export_assembly` publishes a file even though it preserves document; writer | Captures under engine lock, then awaits `spawn_blocking(write_export)` through staged/current-native protected publication |
| `fabrication::register` | `export_fabrication` publishes release; writer | Awaits captured bounded snapshot `spawn_blocking(snapshot.write)` through all publication |
| `output_jobs::register` | Load settings/inspect release observations; save settings/run release writers | Every `spawn_blocking` is directly awaited through settings/publication completion; inspect reader has its own retained permit |
| `symbol_libraries::register` | Snapshot/load observations; detached save/export writers | Directly awaits `spawn_blocking` through synchronous guarded library publication |
| `agent_presence::register` | Query observation; set/clear session attention are writers | Context-owned identity captured into directly awaited blocking worker; no document mutation but writes session metadata |
| `pcb_branches::register` | List/diff observations; create/apply/inspect/discard explicit private candidates; exact merge remains live writer | Directly awaits blocking operation. Create/apply additionally move existing nonwaiting branch permit into actual worker. Original connection/creator ownership and exact cursor guards remain |
| `views::register` | Board SVG observation only without top-level path; with any path present writer in Live, forbidden in restricted modes | Capture plus directly awaited blocking render and optional SVG publication |
| `png_views::register` | Board PNG same path rule | Semaphore acquired before capture and retained inside directly awaited blocking render/publication |
| `schematic_views::register` | Both SVG/PNG same path rule | Shared schematic semaphore retained inside directly awaited blocking render/publication |

All these production registrations have an awaited completion boundary. Test-only `std::thread::spawn` barriers are not detached production handlers. The existing transport's `JoinHandle` or request future can be dropped before its blocking child completes; that fact is why registry admission must retain the complete handler independently of the transport runtime. A private process-lifetime executor must finish these awaited chains before dropping its writer token. No general handler-completion claim is inferred from its observation annotation or name.

## Additive package-stack audit

Read against T116 immutable product source `37f1a654ffbb0b253393c12fb6a26bd671ccf9cd`, including T114 and T115. The final combined gate remains separate from this source audit.

| New registration or command | Trusted admission | Actual completion boundary |
| --- | --- | --- |
| `symbol_packages::register`: `load_symbol_package`, `query_symbol_packages` | Explicit observations; bounded regular-file read or schematic-only query | Each handler directly awaits its `spawn_blocking` worker, then serializes the result. No publication or project mutation. |
| `symbol_packages::register`: `save_symbol_package` | General registration defaults to writer; hidden and refused in Approval and ReadOnly | Borrowed path/unit/pin admission precedes typed decoding. The handler directly awaits `EngineHandle::save_symbol_package`, including synchronous private staging and lifecycle-guarded no-clobber publication. |
| Engine host `place_symbol_package`, `delete_symbol_package`, `annotate_schematic` | General host registration defaults to writer | `host::blocking` directly awaits synchronous original engine dispatch and transaction completion. Package membership and annotation guards remain inside that original dispatch. |
| Engine host `preview_schematic_annotation`, `preview_package_pcb_component` | Explicit names in `EngineHost::is_observation` | The same awaited blocking dispatch completes bounded capture, original cursor/identity checks and preview construction without committing the proposal. Returning a guarded command never admits that command as an observation. |
| Engine host `create_package_pcb_component`, `link_schematic_package`, `synchronize_schematic_pcb` | General host registration defaults to writer | The same awaited blocking dispatch retains the original whole-project/full-cursor guards through the synchronous transaction. One physical package owner does not change admission. |

Source paths: `crates/mcp/src/symbol_packages.rs`, `crates/mcp/src/engine_host.rs`, `crates/mcp/src/host.rs`, `crates/engine/src/package_component.rs`, and the original package/annotation/sync command implementations. No new production worker detaches from its returned handler future. The registry's retained executor therefore owns the entire awaited publication/transaction chain after caller cancellation, as for the original registrations.

## Verification obligations for the executor

Use actual default-deny counter handlers and held blocking workers; admit at most 16 writers without queuing. Preserve original CallContext and native/library publication guards. Cancel the caller, close its real session and drop the endpoint runtime while a worker is held; policy must remain busy until it finishes. Test panic and rejected admission without token leaks. Every Live mixed view with any path presence must enter this same retained path, including malformed values; restricted path refusal must occur before handler entry. Re-audit new package save/export/create/link/annotation/sync registrations on final integration and preserve their independent predecode bounds.
