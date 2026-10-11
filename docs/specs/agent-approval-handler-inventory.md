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

## Verification obligations for the executor

Use actual default-deny counter handlers and held blocking workers; admit at most 16 writers without queuing. Preserve original CallContext and native/library publication guards. Cancel the caller, close its real session and drop the endpoint runtime while a worker is held; policy must remain busy until it finishes. Test panic and rejected admission without token leaks. Every Live mixed view with any path presence must enter this same retained path, including malformed values; restricted path refusal must occur before handler entry. Re-audit new package save/export/create/link/annotation/sync registrations on final integration and preserve their independent predecode bounds.
