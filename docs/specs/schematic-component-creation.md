# Explicit linked PCB component creation

## Public behavior references

The [KiCad 9 forward annotation manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#forward-and-back-annotation)
describes adding PCB components using schematic identities and assigned footprints.
Its [public footprint format documentation](https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/index.html)
distinguishes reusable local pads from placed component geometry. These behavior
references guide an independent native implementation. No external implementation,
library assets or file compatibility are copied or implied.

## Contract

An explicit action creates one PCB component for one existing, unlinked native
schematic symbol. The caller supplies a validated Viamind rectangular SMD footprint,
integer nanometre placement, board side and quarter-turn rotation. A symbol's
nonempty footprint assignment must match the supplied footprint name. An absent
assignment permits an explicit chosen footprint without editing the symbol.

The symbol and footprint must have a nonempty, exact, unique pin/pad-number
bijection. Each pad receives the same reserved `sc:<graph-component-identity>`
net used by forward annotation, including isolated pins. Visible labels do not
replace stable net identities. Existing same-net copper may connect the newly
created pads; its geometry and assignments remain untouched. This action does
not certify clearances, placement legality, fabrication readiness or continuity.

A read-only preview captures the same bounded project/cursor used by synchronization,
then validates and computes outside the engine mutex. It returns an explicit
command and inspectable reference, value, footprint, placement and numbered net
assignments. New part/pad UUIDs are allocated only when applying a valid command.
A duplicate board reference or existing symbol association rejects; links are
never inferred from reference text. Existing model data remains byte-for-byte
equal except the addition of one fully linked Part.

The command requires the exact captured project and event cursor. Stale design,
same-project reopen, malformed footprint, ambiguous pin numbering, unknown symbol,
footprint mismatch, bounds or collision failures leave model, selection, dirty
state, history and events unchanged. Application validates before recording one
part delta and one undoable transaction. Undo removes only that component; redo
restores its exact identities. Batch execution retains the engine's initial-cursor
contract and rollback semantics.

Input project limits are synchronization's 10000 objects/4 MiB aggregate text,
4096 bytes/string and 8 MiB serialized guard; footprint limits remain 1024 pads
and 1 MiB library file. Resulting project must fit the same limits. Origins and
all transformed pad corners must stay within ±10000 mm, checked in integer
arithmetic before geometry commits. Rotations are 0,90000,180000,270000 mdeg;
Bottom mirrors local X before rotation, matching existing Part conventions.

## Desktop and MCP

Expose a shared catalog command and a read-only MCP preview. Desktop provides an
explicit symbol selector, library path, integer nanometre coordinates, side and
rotation, a Prepare button, an inspectable preview and an explicit Create button.
Capture, file loading and preview calculation use one retained background worker;
closing the dialog or replacing a document cannot start additional workers or
apply an old result. Form changes invalidate previews, fresh cursor is required
for creation, and document replacement clears old choices. All mutation uses
the shared action catalog. A small original footprint fixture illustrates the
native format; arbitrary file/library searching is outside this increment.

## Verification

Independently verify regulator VIN/3V3/GND groups across creation of its three
symbols, exact placement and bottom rotations, preservation of existing copper
and unrelated content, atomic undo/redo and native/log schema6 persistence.
Exercise duplicate references/links, malformed mapping, mismatch, transformed
corner overflow, stale sequence/stream and batch rollback. Use actual egui inputs,
two real MCP HTTP clients, production native stdio/HTTP, readable Linux capture,
and full Linux fmt/strict clippy/tests/build before review.
