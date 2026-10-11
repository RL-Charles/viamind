# Checked walkaround through one fixed via

Use **Board → Walkaround through fixed via…** for opposite-side same-net explicit
pads. Set grid, track width, via copper diameter/drill and clearance settings.
Choose both pads, then click empty grid space for one fixed through-via center.
Request **Plan static walkaround** explicitly. A successful joint checked preview
shows the start-side leg in orange, target-side leg in blue and the proposed via.
Finish or Enter recomputes both legs plus the via and commits them in one undoable
operation. Escape cancels locally. **V** or **Place via** clears the checked plan
and lets you choose another fixed center. Manual interior vertices are unavailable
in checked walkaround mode; existing manual through-via routing remains separate.

Agents use the same typed input for read-only `plan_pad_via_walkaround` and
undoable `route_pad_via_walkaround`:

```json
{
  "expected_project": "PROJECT_UUID",
  "expected_cursor": {"stream_id": "STREAM_UUID", "sequence": 0},
  "start": {"part_id": "PART_UUID", "pad_id": "PAD_UUID"},
  "target": {"part_id": "OTHER_PART_UUID", "pad_id": "OTHER_PAD_UUID"},
  "via": {"x": 30000000, "y": 10000000},
  "grid": 1000000,
  "width": 250000,
  "via_diameter": 600000,
  "via_drill": 300000
}
```

Obtain actual IDs and the full cursor from `query_route_endpoints`; all geometry
is integer nanometres. The via center must already be on the requested grid.
The immutable response includes both exact paths, derived layers/net, fixed via
geometry, per-leg/aggregate work, source cursor and complete scoped preflight.
Changing the design or reopening a project invalidates the old request. Commit
recomputes from the guarded current snapshot, adding all tracks and exactly one
via together; one undo removes them and redo restores their stable IDs.

## Bounds and refusals

One admission and one bounded board-only snapshot cover both legs. The aggregate
limits are 64 relevant boxes, 20000 graph nodes, 100000 candidate edges,
4000000 intersection tests, 510 emitted interior coordinates and 1024 new tracks
plus one via. Boxes needed on both sides count twice. Budgets are charged before
graph allocation/intersections; exceeding them refuses the entire proposal.

The board outer outline and every substrate cutout must be valid axis-aligned
four-corner rectangles. Exact normalized quarter-turn explicit pads are required;
route-side legacy pads/unsupported transforms refuse. Substrate holes use route
half-width plus **edge** clearance and one strict Nm. Foreign/unassigned copper
uses copper clearance; every existing via drill hole is excluded regardless of
net. Exact off-grid pad stubs remain attached and may be non-cardinal. Closed
conservative boxes can refuse physically clear paths.

Success requires complete shared scoped DRC of both paths AND the new via,
including annular, drill, edge and physical hole spacing. A position with clear
track centerlines can still refuse because the via does not fit. No successful
single leg is published as a checked route. Existing unrelated violations and
documented checker exclusions still apply. Fixed positions can have no route;
change the selected center or use manual routing.

There is no automatic via placement, shove, existing copper movement/deletion,
general polygon/45-degree walkaround, multiple transition, inner-layer,
blind/buried via, differential/plane routing or physical certification claim.
Conservative stored-endpoint connection-guide proofs are separate. Strict native
100k-object60fps remains open.
