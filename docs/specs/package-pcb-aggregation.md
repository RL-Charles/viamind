# Complete explicit schematic package to one physical PCB component

Ticket T-0116, parent S-047. Initial isolated stack base is T114
`0aebb59d11914a82d1e0edfd45531a27e340db40`, with model foundation
`928bbdd`; only this ticket's commits will be transplanted onto the final
T114/T115 successor before combined verification. T112 has been submitted
and frozen. No Rust implementation precedes independent approval of this
public behavior specification. T115 owns shared-reference annotation.

## Public behavior sources

- [KiCad 9 Schematic Editor — creating and editing symbols](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols),
  read 2026-10-11. Multiple units can describe one package and have different
  pin layouts. The broader shared-pin and interchangeable-unit behavior is
  outside this increment.
- [KiCad 9 Schematic Editor — forward and back annotation](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#forward-and-back-annotation),
  read 2026-10-11. Stable internal identities retain schematic/footprint links
  across reference edits. Forward and back updates are explicitly prepared
  before applying changes.
- [KiCad 9 PCB Editor — forward and back annotation](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#forward-and-back-annotation),
  read 2026-10-11. Board fields can update linked schematic fields; choosing
  reference-based relinking is a separate operation.

Only these observed public contracts are used. Viamind's types, limits,
identities, refusal behavior and verification fixture are independently
designed. No library data, proprietary code, artwork or manufacturer package
geometry is copied. No vendor placement orientation or electrical equivalence
claim follows from package membership.

## Exact scope

Create one physical PCB Part from one complete explicit SchematicPackage,
whose member units all exist on one sheet. Require one supplied rectangular
SMD Footprint with a nonempty exact bijection between all package pin numbers
and explicit footprint pad numbers. The physical origin, side and cardinal
rotation are entered explicitly; no unit origin or body center is substituted.
All 14 pins of the authored four-functional-unit plus separate-power-unit
fixture map exactly once to 14 distinct physical pads. Roles never join nets.

Add explicit package-to-existing-Part linking/unlinking; integrate complete
packages into guarded forward/back synchronization and component/pin-pad
queries/cross-probing. Preserve ordinary single-unit creation/linking and
exports. Do not infer a package from shared references, unit letters, values,
pin names, footprint names or proximity. Exclude partial packages, multiple
physical parts per package, shared/repeated pin numbers, unit swapping,
cross-sheet membership, hidden power, net migration through routed copper,
footprint replacement, implicit component deletion and vendor certification.

## Native identity and compatibility

Proposed optional scalar Part.schematic_package: Option<ObjectId>, omitted
when None, is mutually exclusive with schematic_symbol. Its target is the
explicit SchematicPackage.id, never an arbitrarily selected member symbol.
A package has at most one linked Part. Grouped symbols still cannot be targets
of ordinary schematic_symbol links. Nonnil IDs, global object uniqueness and
wrong-object-kind checks remain mandatory.

Reserve native/event feature 12 only after independent review. Compute the
maximum of every existing feature floor, including package membership 11,
via minima 10, assembly calibration 9 and sourcing 7. Absence preserves all old
native/export bytes and previous feature floors. Event base and every Part
before/after snapshot containing a package link require the new feature, even
when a later record removes it. Older envelopes/log headers refuse it.

As with existing unresolved symbol links, removal of the schematic package
may retain an unresolved stable package ID on the physical part. Observations
must identify unresolved coverage; create/link/sync cannot silently repair or
rebind it. Recreating a package with the same reference does not recreate its
old identity. Explicit unlink is required before reusing the physical part.
Deleting a physical Part leaves its schematic package intact and unlinked.

Audit every Part constructor, clone_without_assembly and purpose-specific
capture; fixed scalar links must survive geometry clones without copying
unused sourcing text. Add None to literal constructors. No new allocation
comes from the scalar alone. T113's custom BorrowedPart serializer must
include the new scalar in exactly native field order with identical skip
behavior; its pre-copy 2 MiB payload budget must charge it. Byte-equivalence
tests include absent, single-symbol and package-link variants. Package
membership counts and strings remain
charged before map/vector/Project clones, including current/expected guards,
mapping queries, graph and cross-probe snapshots. Retain current bounded
sync admission (10000 inspected objects/pins/outline vertices, 4096 bytes per
string, 4 MiB aggregate text, 8 MiB serialized guard) and source full cursor.
Count all package records/members and their keys/names/template strings.

## Guarded create/link operations

Separate typed shared package create/preview API uses expected project UUID,
full source cursor and explicit package ID, footprint and physical pose. The
prepared command contains the same bounded exact Project snapshot used for
its pin/graph calculation. Apply revalidates both borrowed current/expected
budgets, exact project equality and full stream/sequence before allocation
of final identities or mutation. Dynamic and batch calls use the same engine.

Validate complete membership, equality of member reference/value/footprint,
unique full package pin numbers and exact pad-number set before creating one
Part. Reject existing package link, duplicate physical reference, absent or
extra pad, unsupported geometry/rotation, mismatched assigned footprint,
incomplete graph or transformed-corner overflow. Use existing exact doubled-Nm
pad-corner bounds and local-X bottom reflection followed by cardinal rotation.

For each explicit unit pin UUID obtain its existing graph net, then assign its
corresponding physical pad the same stable sc: net. Distinct disconnected
pins, including power-unit pins, remain distinct graph nets unless actual
wires/labels connect them. Pad names/roles never create connectivity. One
Part delta is one undo transaction; redo restores the identical physical Part
and all pad/link IDs. No units/schematic geometry or routed copper changes.

Explicit linking uses whole original Part plus project/full-cursor guards and
the complete expected package record/member symbols. It rejects any existing
single-symbol or other package ownership conflict, wrong-kind/missing target,
partial terminal mapping and mismatched footprint. Explicit unlink clears
only the selected link and preserves Part pose/pads/nets/assembly. Ordinary
single-symbol link commands refuse an already package-linked Part rather
than accidentally leaving both link fields set.

## Forward/back synchronization

Existing preview/apply uses the same single captured immutable Project and
full cursor. A package represents one physical owner; all its member symbols
are excluded from the ordinary unlinked-symbol list only when their explicit
package is linked. Shared reference/value/footprint source consistency is
validated, never taken from an arbitrary differing member.

Forward copies the package's shared reference/value to its one Part and derives
each pad net from the pin's actual member symbol and sheet. Exact complete
pin/pad coverage and matching footprint assignment are mandatory. Existing
routed-net reassignment refusal applies to every package pad. Back copies the
Part reference/value/footprint to every member symbol atomically. Geometry,
membership order, unit IDs, pin IDs, roles, assembly metadata and all copper
remain unchanged. Reference collisions with other packages/ordinary symbols
or physical parts reject the entire plan. No new inferred link or creation.

Prepared change rows identify package/unit/symbol/pad provenance where relevant;
one physical field change must not be presented as several physical components.
One synchronization undo restores all member fields and physical nets exactly.
Stale edit/undo/reopen cursor, wrong expected snapshot, malformed/overbudget
package and any refusal leave complete session/history/selection/native bytes
and peer edits unchanged.

## Read-only mapping and human workflow

Extend bounded component mapping with omitted optional package metadata and
an ordered list of actual unit IDs/keys/sheet/positions. Query by Part ID,
package ID or member symbol ID resolves the same physical owner and complete
unit set. Preserve ordinary row bytes where new fields are absent. No duplicate
physical components appear just because a package has five visible bodies.

Pin-pad mapping resolves all package pins and returns each exact source
package/unit/symbol/pin identity plus physical pad identity/position, side,
coverage and unsupported reasons. Existing 1..100 paging and inspected-object
text limits apply before row cloning. Add no electrical continuity inference.
Cross-probing a unit highlights its one physical Part; choosing the Part exposes
all units and exact terminal counterparts. Any navigation anchor is explicitly
a display aid, never a manufacturing centroid or inferred physical origin.

Desktop creation chooses an explicit complete package separately from ordinary
symbols, displays shared reference and full unit/pin count, enters the native
footprint path and physical pose, and prepares the shared command off the UI
thread with one retained fallible named worker. Source replacement/full-cursor
changes invalidate the prepared command; Close retains the pending receiver.
Bound path inputs before ownership and disable busy edits/duplicate workers.
Show exact complete pin/pad preview and DRC-after-placement limitation before
Apply. Compact 640x400 actions/results/errors remain reachable through scroll.
The existing synchronize and cross-probe controls share the same engine APIs.

## Required evidence

Independent approval precedes model edits. Tests cover authored 5-unit/14-pin
package creation with one physical Part, independent numbered-terminal oracle,
isolated power nets vs actual contacts, every missing/extra/duplicate/mismatched
mapping, stale two-client guards, routed-net refusal, all-member backward fields,
forward updates after shared annotation, explicit link/unlink and unresolved
identity. Native12/maxfeatures/older-header refusal, base/before/after replay,
undo/redo/reset and ordinary no-link byte regressions are required.

Exercise typed/dynamic/batch engine paths, real initialized two-client HTTP and
stdio, read-only observation whitelist, actual compact human prepare/create/
sync/cross-probe/error inputs, and self-closing native framebuffer evidence with
honest local-input provenance. Retain source/binary/report/fixture identities,
full-session/native invariants and independent production rerun. Run four Linux
gates, actual canonical catalog/drift/workflow and fresh exact-head CI. No story
completion until independently reviewed; only a human marks work done.
