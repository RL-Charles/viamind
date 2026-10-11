# Guarded schematic and PCB synchronization

## Public behavior reference

The [KiCad 9 Schematic Editor manual, forward and back annotation](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#forward-and-back-annotation)
describes stable identity links, an inspectable change list before applying, and
transfer of references, values and net connections to the PCB. Its reverse
workflow can transfer selected component properties back to symbols. These
behaviors guide this independently designed native workflow. No implementation
code or proprietary artifacts are used.

## Initial supported scope

Link existing explicit PCB components to existing native schematic symbols by
persisted symbol UUID. Reference changes never establish or replace a link
implicitly. Linking and unlinking use exact project/part/symbol guards and shared
undo. One symbol can link to at most one board part. Deleting a symbol can leave
an unresolved link, which synchronization reports rather than silently
reassigning. Deleting its board part removes that part's link with the part.

Forward annotation updates linked part references and values and assigns every
explicit pad from the matching symbol pin number and exact schematic graph.
Pin/pad numbers must form an unambiguous bijection. Footprint geometry is retained;
an incompatible footprint assignment rejects with guidance to apply appropriate
geometry explicitly. No part creation, removal or inferred footprint lookup.
Unlinked symbols and board parts are counted and listed as exclusions.

Physical net names use a deterministic reserved schematic component identity;
the preview also shows the graph's canonical label and aliases. This avoids
joining two sheet-local nets which share a visible label. Every linked pin,
including an isolated pin, receives its exact component identity. Copper net
changes require special care: the initial implementation rejects pad net changes
that would orphan or reassign existing tracks/vias. It preserves all copper.

Back annotation updates linked symbol references, values and optional footprint
assignments from PCB component properties. It retains pin IDs/types/numbers,
symbol geometry, wires, junctions, labels and connectivity. Reverse net-name or
topology editing is excluded. Reference collisions reject the complete operation.

## Preview, bounds and application

A read-only preview captures bounded relevant schematic and board data with a
project identity and source cursor. It calculates outside the engine lock and
returns deterministic changes, exclusions and a literal guarded command.
Application checks the captured inputs atomically, recomputes its changes from
validated native data and commits one undoable transaction. A stale preview or
invalid link/mapping changes no model, selection, history, dirty state or events.
No-op application leaves the session unchanged. Same-project reopen invalidates
desktop previews through the event stream identity.

Budget inputs before copying or allocating: exact graph limits are retained;
linked part/pad counts, aggregate text and serialized command size receive
explicit additional limits. Oversized input rejects instead of partially
synchronizing. Reports distinguish supported scope from a complete netlist or
complete project synchronization claim.

The native format must preserve links, read previous schema versions and reject
invalid duplicate/non-symbol identities without inventing links. Shared engine
commands, desktop actions and MCP tools use one behavior and one undo history.

## Verification

Use original geometry with two same-named local nets on separate sheets and a
global label, rotated/mirrored symbols and explicit numbered PCB pads. Verify
stable linking across reference renumbering; exact forward pin/net assignment;
reverse properties without geometry/connectivity loss; duplicate/missing pins,
footprint mismatch, orphan link and routed-net changes; stale preview and budget
rejection; one undo/redo; native persistence and old-version compatibility; and
actual production MCP plus desktop input flows on Linux.
