# Conservative exact endpoint copper connectivity

## Public behavior source

[KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#net-and-net-class-controls) describes ratsnest lines between unconnected items and distinguishes their visibility from electrical checks. The same public manual describes tracks, vias and pads as copper connections. Viamind independently implements the narrower proof below from its native model; no external source code or assets are used.

## Required behavior

1. Preserve named explicit-pad membership and existing stable terminal ordering. Derive a conservative forest of proven electrical islands from native straight tracks, rectangular SMD pads and through vias. Every join requires an exact case-sensitive net match.
2. A valid straight track joins its two exact integer endpoints on its outer copper side. Tracks join only at identical endpoints on the same side. No proximity tolerances, float equality, interior crossings, T intersections, capsule contact or pad-edge contact are inferred.
3. Supported quarter-turn/bottom-mirrored pad centers bind to an existing same-side copper endpoint at exactly the transformed integer center. Legacy preview pads, arbitrary-angle pads, invalid geometry and out-of-range positions cannot provide a proof. Coincident pads without a copper endpoint remain conservatively separate.
4. A valid through via bridges matching top/bottom endpoint nodes at its exact center. On each side an incident segment must extend beyond half the drill diameter so the proof never relies on copper entirely inside the drilled hole. This extra conservative condition may leave guides on very short but physically overlapping shapes.
5. Build the historical deterministic terminal tree, then retain only edges that join separate proven islands, updating the island forest after each retained edge. Output contains exactly one fewer guide than the remaining proven islands per nonempty net; stable identities and existing small-net Prim/large-net coordinate-chain ordering are preserved. No copper or document state changes.
6. Use the same result for visible board guides, route-guide identities, named-net summaries and connectivity MCP results. Include additive proof metadata in queries: mode, supported/unsupported named pads, analyzed tracks/vias, and budget fallback. Say these are conservative guides, not a complete unrouted or DRC certificate.
7. Bound proof work before allocating a graph: at most 100000 combined parts/pads/tracks/vias, 4096 bytes per inspected net/name string and 2 MiB aggregate inspected strings. Exceeding the budget disables all suppression, preserving historical guides and explicitly reports fallback. No model/schema changes or new dependencies.
8. Verify same-side routes, disconnected partial chains, exact one-nanometer gaps, wrong nets/sides, arbitrary transforms, midsegment crossings, mirrored off-grid quarter-turn pads, valid through-via bridges, drill-contained segments, unchanged session/cursor/history, route/undo/redo, native save and real independent MCP clients. Full Linux gates and separately labeled production native evidence precede merge.

## Exclusions

No filled planes, inner layers, arbitrary rotation proof, midsegment contacts, overlapping shape/annulus analysis, dangling-track certification, clearance guarantees or physical fabrication certification. Conservative extra guides are expected outside the exact-endpoint subset. Existing historical verification reports retain their source provenance.

## Review refinement: drilled copper

A reviewer supplied a valid tiny pad/track example wholly removed by a through-via drill. Endpoint topology alone cannot prove that path. Before graph construction, index all validated via drill bounding boxes, including unassigned/foreign nets. Reject proof of any track centerline bounding box intersecting a hole unless it is an exact endpoint at an isolated same-net valid via and its segment reaches the annulus. Pad centers inside/touching hole boxes cannot bind. A via whose annulus box intersects another hole cannot prove a bridge. Invalid via geometry disables suppression. Bounding-box over-exclusion deliberately retains extra guides. At most one million aggregate spatial candidates are examined, otherwise the entire proof falls back. This refines the original conservative subset without changing document geometry.
