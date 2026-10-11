# Conservative remaining connection guides

Cyan PCB guides, route-guide picking, `query_nets` and `query_connectivity` now share
bounded exact-endpoint copper proofs. A same-net track chain connects its exact
integer endpoints on one outer copper side. An explicit quarter-turn/mirrored
SMD pad binds at its exact center. A through via bridges top/bottom endpoints at
its center when both incident segments reach beyond its drill radius.

After joining these islands, the historical deterministic terminal tree supplies
one fewer remaining guide than the island count. Undo, redo and moving/removing
copper recompute guides from the current document. Read-only queries preserve
all document and history state. Membership, ordering, pagination and terminal
coordinates remain unchanged.

Both net queries return additive whole-board `copper_proof` metadata: supported
and unsupported named pads, analyzed tracks/vias, unsupported named copper and
`budget_fallback`. This is conservative routing guidance. It does not certify
physical continuity, clearance, dangling tracks or fabrication readiness.

Midsegment crossings/T contacts, pad-edge and capsule contact, arbitrary pad
rotation, planes and inner layers cannot prove connections. Extra guides remain
for them. Coincident pad centers without a track endpoint also remain separate.
A segment wholly inside a via drill cannot prove that bridge.

Proof work is bounded before graph allocation to 100000 combined board
parts/pads/tracks/vias, 4096 bytes per inspected string and 2 MiB inspected text.
Exceeding a bound disables all suppression and reports `budget_fallback: true`;
the historical conservative guides remain. No native schema changes.
