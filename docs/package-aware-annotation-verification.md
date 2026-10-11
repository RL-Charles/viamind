# Package-aware schematic annotation verification

## Supported behavior

`preview_schematic_annotation` plans references by explicit owner: one ordinary
symbol or one stored multi-unit package. A package consumes one number, and all
its units (including explicit power units) retain the same reference. The anchor
is the member with the lexicographically smallest unit key. Reference allocation
uses the chosen X/Y order and complete sheet order; default sheet order is UUID
order. Existing retained spelling is preserved. Numeric aliases ignoring case
and leading zeros conflict across owners.

The desktop's **Annotate symbols** entry opens the shared review. Prepare leaves
the complete project and document status unchanged. Apply submits the exact
preview command, guarded by full session cursor and complete original sheet map;
one Undo restores every affected unit. Option edits and Cancel discard only the
local proposal. Same-project reopen clears the review, and peer edits disable
stale Apply. Ordinary old callers may explicitly omit the cursor; grouped callers
require it, and all new previews include it.

## Bounds and coverage

100 sheets, 2000 actual symbols, 10000 combined schematic structural rows/pins/
package members, 256 KiB known schematic text, 2 MiB serialized expected sheets,
and a separately bounded 2 MiB + 8 KiB whole command envelope. Borrowed capture
checks precede sheet copies. Raw decoded JSON is checked before secondary typed
sheet allocation on registry/MCP dispatch; this does not cap the original
transport JSON decoding. The annotation invariant checks the existing schematic
identity namespace, seeded by project identity, without revalidating unrelated
PCB identity or assembly content. UUIDs, geometry, pins, wires, roles, membership,
values, footprint fields and PCB remain unchanged. Package PCB aggregation,
hierarchy and automatic reference prefix inference are outside this increment.

## Evidence status

Source `272d5d2f39dc6cb8feecd9aad5d00d0fe8bc36a9` passed all Linux gates:
fmt, strict workspace all-target Clippy, **819 passed / 0 failed / 1 ignored**,
workspace build and inspection example build (session59228). Prior source
`db1d596` passed30 focused checks:5legacy+9package+6inherited package+2bounds+
2actualHTTP+6actualUI. The sole later code change is a direct cursor accessor
for strict lint. Historical failed attempts remain separately indexed. The authored fixture uses distinct unnumbered spellings
U?, u?, U and u for separate owners, preserving the foundation's native exact
reference uniqueness across owners.

Actual retained production `0ccd0d0e75ee3ed0718e71473994da8919225d6634c35731c151830cfc509a93`,
PID2463504, serves7945. Two initialized clients passed **29 checks**, including
whole-package apply/oneundo/redo, exact peer reads, stale sequence/reopen,
malformed raw guards and schema11 round trips. Actual headless readonly7checks
passed. One12-unit/two-package Apply measured20.33ms under current load; this is
a fixture observation, not a worst-case latency claim.

[Master](verification/t0115/master.json) indexes durable logs, drivers, exact
source and binary hashes, launch metadata and native outputs. Every indexed
artifact and original raw RGBA/RGB capture can be independently verified.

## Native inspection provenance

Inspection-only hosts drive the production ViamindApp with actual egui option,
text and button events, then display its uncommitted shared preview using the
native WGPU renderer. Full session/status invariants are checked each UI pass.
Prepare/Apply/Cancel bounds use exact labeled widget IDs and the current native
read_response rectangles. Hosts self-close after exclusive-created, fsynced
Screenshot-event outputs. Original Color32 RGBA is retained as P7; the RGB-only
review PNG preserves every corresponding sample and omits alpha explicitly.
UI passes are callback counts. Logical focus retention is an inspection aid;
there is no OS-input, compositor-capture or frame-cadence claim.

Wide1600x1000 and compact640x400 hosts self-closed after12/11UI passes.
Actual framebuffer sizes were2667x1667 and1067x667. All native action rectangles
fit the production window/content. Both review PNGs were viewed: options, starting
number, package/member count, unit references and Prepare/Apply/Cancel are
readable. Long UUID metadata truncates to one row; full metadata remains in
its tooltip/JSON, and actual compact scrolling reaches the final member.

![Wide shared package reference review](verification/t0115/annotation-wide.png)

![Compact shared package reference review](verification/t0115/annotation-compact.png)

## Final integration

The original proof source above remains fixed. Final transplant onto the literal
T114 evidence head, actual combined tool catalog generation and fresh exact-head
CI are pending; no final combined native/source claim is inferred.
