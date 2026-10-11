# Manufacturing export snapshots and limits

CSV and scoped Gerber/XNC exports capture only the data they use. Assembly takes
component ID/reference/value/footprint/origin/rotation/side, excluding pads,
outline, copper and schematic links/content. Fabrication takes project identity,
name and board geometry in an empty schematic shell; unrelated sheets and their
IDs do not participate in board validation.

Both capture source cursor and active native path under the same engine lock.
Expected-cursor guards are checked first. Count and text budgets are checked
before copying. Rendering and publication run outside that lock. Desktop
fabrication's initial cursor read also avoids copying the session.

| Limit | Assembly CSV | Gerber/XNC package |
| --- | --- | --- |
| Components/primitives | 100000 component headers | 200000 budgeted primitives, including part headers/rings |
| Per copied UTF-8 string | 4096 bytes | 4096 bytes |
| Aggregate copied UTF-8 text | 2 MiB | 2 MiB |
| Published output | 32 MiB CSV | 128 MiB total package |

Fabrication counts pads twice (copper/mask), vias five times, tracks once, and
outline vertices/rings plus component headers. Empty lists/components still count
where needed to bound allocations. Existing geometry/coordinate/drill restrictions
remain in force; no new manufacturer format or fabrication certification is added.

Two manufacturing snapshots may exist per process. Admission is nonblocking and
shared by library, CLI, desktop and MCP captures. A busy capture returns an error;
retry after another export finishes. Private snapshot types own their admission
permit until dropped. Production blocking workers retain the snapshot throughout
render/write, so cancellation of the awaiting client cannot release a running
worker's slot. Callers retaining a library snapshot also retain its slot.

Library consumers can capture `AssemblySnapshot` or `FabricationSnapshot`, then
render/write from the immutable snapshot. `csv` is now fallible; the existing
write-function signatures are preserved. Accepted outputs retain their exact
bytes. CSV still requires explicit overwrite permission; fabrication always
requires a NEW directory. Active native protection and atomic publication remain.
Budget/busy/stale failures create no output and leave design/history/status intact.

The limit regressions include huge invalid unrelated sheets, malformed large pad
metadata excluded by assembly, UTF-8 byte boundaries, aggregate text/component
counts, process admission release and canceled blocking-worker ownership.

[Linux, production CLI/native and compatibility verification](manufacturing-export-verification.md).
