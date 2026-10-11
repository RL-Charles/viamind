# Explicit-pad connectivity and live connection guides

Public behavioural source: [KiCad PCB Editor 8 manual](https://docs.kicad.org/8.0/en/pcbnew/pcbnew.html), Board connectivity and ratsnest display. The public manual describes nets as electrical relationships between pads and the ratsnest as visible outstanding connections. This independent implementation uses that observable behaviour; no source implementation is used.

## Behaviour

Only persistent explicit pads with a case-sensitive assigned net name participate. Legacy footprint previews, unassigned pads and empty explicit pad lists invent no electrical connectivity. Same-numbered pads have no implicit internal short. Each pad is a distinct required terminal, including coincident pads. No tracks, vias or zones exist in the current model. A net of N pads produces N-1 required terminal-to-terminal guide edges, a conservative estimate of routing work. Touching or overlapping same-net copper pads can already conduct; this increment does not analyze copper continuity and may therefore overestimate unrouted work. These are guidance, not copper, clearance checks or proof of manufacturability.

A shared footprint transform mirrors local X on the bottom side, rotates counterclockwise in the existing y-down board coordinate system, then translates. All stored geometry remains integer nanometres; transformed output coordinates are millimetres at the query/render boundary.

`query_nets` returns a name-sorted paginated summary of assigned nets, terminal counts and guide counts. `query_connectivity` requires an exact net name, reports full terminal/edge counts and the algorithm, and returns separately paginated terminal and guide arrays using the same offset and limit (default100, maximum1000). Missing net names return empty results. Results reflect one consistent snapshot and emit no transactions, selection changes or events; repeat pages only while the design is unchanged.

For at most256 terminals per net, deterministic dense Prim computes a minimum spanning tree using Euclidean centre distances (O(N²) time, O(N) storage). For larger nets, two coordinate-sorted chains are evaluated and the shorter chain selected, X ordering winning ties. This deterministic approximation uses O(N logN) time and O(N) storage, avoiding a complete edge graph. It is not a minimum spanning tree; guide lines may cross, pass through obstacles and need refinement by future routing. Identity breaks geometric ties. Each algorithm produces exactly N-1 edges, preserving coincident distinct terminals.

Desktop scenes derive the same tree from the same model and draw one-pixel cyan guides independently of copper layer visibility. They rebuild on every design event including move, arbitrary rotation, side flip, net assignment, delete, undo, redo and project replacement. Guides have no editable model identity or saved state.

## Checks

Verify net exclusion/case sensitivity, deterministic order, bounded pagination, both algorithm branches, a tree invariant and known minimum length, read-only event/history preservation, shared geometric transforms, HTTP multi-agent reads after edits, and actual desktop scene refresh after edits and undo.
