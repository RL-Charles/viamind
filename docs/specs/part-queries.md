# Board part queries

## Public sources

- [MCP tools](https://modelcontextprotocol.io/specification/2025-03-26/server/tools): discovery, JSON input schemas and tool errors.
- [KiCad 9 PCB Editor search panel](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#search-panel): public context for finding board objects by their fields. No implementation code used.
- The query contract below is an original Viamind design.

## Contract

`query_parts` is a read-only tool on the live desktop engine. It returns a bounded
page of board parts, rather than the entire project. Every supplied filter is
combined with AND:

- `ids`: optional list of UUIDs; an empty list matches nothing. Missing IDs simply do not match.
- `designator`, `value`, `footprint`: optional exact, case-sensitive string matches.
- `text`: optional case-insensitive substring across designator, value, or footprint. Empty text matches all.
- `side`: `top` or `bottom`.
- `bounds`: `{min_x_mm, min_y_mm, max_x_mm, max_y_mm}` matches the **part origin**, inclusively. Bounds use the same finite ±10000 mm range and nearest-nanometre rounding as placement. Inverted bounds are rejected.

Results contain `parts` (full part records, coordinates in integer nanometres),
`total` (all matches before pagination), and `next_after` (last returned UUID when
more matches remain, otherwise null). Parts are ordered by stable UUID.
`limit` defaults to 100 and accepts 1–1000. `after` is an exclusive UUID cursor;
it need not still exist. Repeat the filters and pass `next_after` as `after` to
read the next page. Pages are individually consistent with the live design;
restart the query if the design changes between pages.

Unknown fields, wrong types, invalid UUIDs, invalid limits and unusable bounds
return actionable tool errors. Successful and failed queries leave the document,
selection, undo/redo, log and notifications untouched. The same typed query is
available to Rust consumers through `EngineHandle::query_parts`.

Nets, tracks and DRC records are not represented by the current document model.
They require future query tools when their models exist. This implementation
does not claim the parent story's unrouted-net acceptance criterion.

## Verification

- Filters, exact boundary coordinates, empty results and pagination are checked against known part records.
- Real HTTP clients discover the schema, query another agent's edits, and receive useful errors.
- Query calls preserve pending redo, shared selection, the audit log and subscriber silence.
