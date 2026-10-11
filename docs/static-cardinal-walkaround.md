# Static cardinal walkaround

In PCB **Route pad connection** settings, select **Static cardinal walkaround**,
then **Start route**. Pick two visible same-side explicit pads with the exact same
assigned net, or pick their connection guide. Click **Plan static walkaround**.
The background worker searches and runs the existing scoped joint clearance and
outline checks on a captured board snapshot. Only a successful checked plan is
drawn. **Enter** or **Finish pad route** recomputes the plan against the guarded
live session and adds the entire route in one undoable operation. **Escape** or
**Cancel pad route** discards the local preview.

The board must have an explicit axis-aligned rectangular four-corner outer
outline with no cutouts. Exact quarter-turn and mirrored pad centers are
preserved; short arbitrary-angle stubs attach off-grid centers to nearest grid
anchors. The grid path uses only horizontal and vertical segments. Manual
vertices are refused in this mode; the existing manual, cardinal/45 posture and
through-via routing tools remain available.

Existing copper stays stationary. Foreign or unassigned pads/tracks/via copper
on the route layer are enclosed by conservative integer clearance boxes. Every
via drill hole contributes an obstacle regardless of net. Width halves round
outward, every box gets one extra nanometre, and closed contact blocks an edge.
Diagonal track boxes and stub boxes can refuse physically clear routes. A
route-side part without explicit pads, unsupported transform, malformed object,
no path or exceeded budget refuses explicitly. No hidden fallback moves copper
or changes the requested tool.

The search uses stable coordinate order and minimum Manhattan distance within
its finite graph. It does not minimize corners or promise a globally shortest
physical PCB route. Planning reserves room for the largest candidate before
cloning bounded board data and never clones unrelated schematic content.

| Limit | Maximum |
| --- | ---: |
| Existing combined board objects before reserved candidate | 98976 |
| Relevant obstacle boxes (copper and holes counted separately) | 64 |
| Cartesian graph nodes | 20000 |
| Candidate undirected edges | 100000 |
| Aggregate node/edge/stub versus box tests | 4000000 |
| Returned interior waypoints | 510 |
| Generated segments | 1024 |
| Retained planning/preflight operations in the process | 2 |

The existing board capture additionally bounds labels and total text. One
desktop worker remains retained after cancellation until it finishes, preventing
repeated cancellation from queueing background searches. Late results are
discarded after endpoint, draft, project or full session cursor changes.

## MCP

Get endpoint IDs from `query_route_endpoints` and use its project/full cursor:

```json
{
  "expected_project": "PROJECT-UUID",
  "expected_cursor": {"stream_id": "STREAM-UUID", "sequence": 12},
  "start": {"part_id": "START-PART-UUID", "pad_id": "START-PAD-UUID"},
  "target": {"part_id": "TARGET-PART-UUID", "pad_id": "TARGET-PAD-UUID"},
  "grid": 250000,
  "width": 250000
}
```

`plan_pad_walkaround` returns immutable checked exact points/stubs, work counters,
derived net/layer and source cursor. It changes no selection, dirty flag, event
cursor or undo history. `route_pad_walkaround` accepts the same input, recomputes
and commits all segments atomically. Optional `rules` uses the existing
`run_drc` rule fields. A peer edit or same-project reopen invalidates the full
cursor; request a fresh snapshot and plan. One undo removes all new segments and
redo restores exact IDs.

Both operations require complete scoped joint DRC, including exact attachment
stubs. Unsafe, incomplete or truncated reports produce no successful plan or
partial route. Existing unrelated violations may remain. The checks cover the
existing explicit pad/track/through-via and outline scope, with that checker's
published exclusions; this is no fabrication or physical connectivity
certificate. Conservative guide suppression uses the separate stored-endpoint
proof, and remaining guide counts retain its limitations.

No shove, 45-degree walkaround, arbitrary outlines/cutouts, new routing vias,
differential pairs, plane routing, worst-case latency or native 60 fps claim is
included.
