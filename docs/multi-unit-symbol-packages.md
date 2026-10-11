# Explicit multi-unit symbol packages

Open **Libraries → Multi-unit package definitions…**, or use the package-library
button in the schematic workspace. This edits a detached native definition;
loading, resetting and saving it leave the project and editing drafts intact.

A definition contains 2–32 ordered rectangular units and at most 1024 explicitly
numbered pins. Add each unit, choose its key/name and functional or power role,
then enter its body dimensions, pin endpoints and electrical types. All units
share a default value and optional footprint name. A power role supplies no
implicit electrical connection. Coordinates and dimensions accept exact mm
values through integer nanometres; sub-nanometre input is rejected.

**Save NEW complete package file** writes a separate
`.viamind-symbol-package.json` definition. Existing destinations and the latest
active native project are protected. **Load complete package file** accepts this
strict native definition format, rather than third-party symbol libraries.

To place a package, choose a schematic sheet and **Refresh complete package
target**. Enter one shared reference and explicit origin, quarter-turn rotation
and optional local-X mirror for every unit. **Place complete package** creates
all units and fresh package/symbol/pin identities in one guarded, undoable sheet
transaction. The completion message lists the package and unit IDs. Refresh
again before another edit. **Delete complete package** removes all its units
atomically and leaves wires in place; undo restores every identity.

The original authored fixture contains A–D amplifier units and a separate power
unit, with fourteen distinct package pin numbers. Stored references remain
shared (`U1`); displayed unit labels identify each member (`U1:A`, `U1:P`). A
placed member's ordinary editor permits pose changes. Shared properties,
geometry and partial deletion require a supported complete-package operation.

Grouped projects use native feature version 11. Ungrouped files preserve their
existing feature floor. Connectivity and ERC inspect actual explicit pins and
expose package/unit metadata. They infer no common pins or hidden power joins.
Package-aware annotation and package-to-one-PCB aggregation are separately
tracked in T-0115 and T-0116; this foundation rejects legacy operations that
would treat one grouped unit as a complete physical component. It does not
claim hierarchical symbols, interchangeable/hidden units, third-party library
import, or physical PCB placement from schematic positions.

See the [public-source contract](specs/multi-unit-symbol-packages.md) for exact
limits and validation rules. The separate single-unit symbol definition format
and valid existing files retain their behavior.
