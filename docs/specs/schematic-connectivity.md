# Bounded native schematic connectivity query

## Public behavior source

Primary source: [KiCad 9 Schematic Editor manual, Electrical Connections](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#electrical-connections), including Wires, Wire Junctions, Labels, and its Connections ERC checks. This public behavior reference describes endpoint contacts, explicit crossing junctions, label anchors and scope. Implementation is independent; no source code or proprietary artifacts were consulted.

## Native behavior

`query_schematic_connectivity` computes a read-only graph for all native sheets in one captured session snapshot. Each wire, junction, label and symbol pin is a graph member, identified by sheet UUID plus persisted object UUID. Pins additionally identify their parent symbol. Symbol bodies do not conduct. Empty legacy sheets have no modeled connections. Board nets, annotation, hidden power pins, buses, hierarchy, net classes and ERC are outside coverage.

Connections use exact integer nanometres, with bounded coordinates and i128 orientation products. There is no proximity tolerance. Symbol pin endpoints use the model's exact local-X reflection followed by quarter-turn transform and translation.

* A wire endpoint on another segment connects both wires, including T contacts and collinear overlaps. Interior/interior crossings remain separate unless an explicit junction touches both segments.
* A pin connects at an exact wire endpoint; a pin merely underneath a segment interior does not connect without an explicit junction there. Coincident pin endpoints connect. A junction connects every segment and pin at its point.
* A label anchor attaches to a segment interior or endpoint, a pin endpoint, a junction, or another label anchor at exactly the same point. Before attaching labels or joining names, touching more than one distinct physical graph component is ambiguous: the whole query returns an actionable error identifying the label and sheet. Add a junction, split/move the anchor, or move the label to resolve this; no partial net report is returned.
* Label names are literal, case-sensitive UTF-8. Same-name labels of any scope connect within a sheet. Across sheets only explicit global and power labels with the same name connect. A local label alone on another sheet does not connect to a global label elsewhere. Pin electrical types do not imply power labels.
* Isolated pins, wires, junctions and labels remain components; this query does not judge whether they are valid circuits. All names are retained as sorted aliases. Canonical display name chooses global, then power, then local, then lexical name. Multiple distinct names are flagged without an ERC verdict. Unnamed components have no canonical name.

Component IDs are the minimum `(sheet UUID, member UUID)` pair, independent of traversal order. Members and nets are ordered by these identities. IDs are stable for repeated snapshots; merging/splitting/editing topology may change them. Pages use offset and limit (default100, 1..100); restart pagination if `source_cursor` changes. The report includes full member identities for returned nets, count/coverage/budget metadata and snapshot project UUID/cursor. It does not change selection, dirty state, undo/redo, model or session events.

## Explicit bounds

Capture refuses more than 1000 sheets, 10000 total sheets/symbols/pins/wires/junctions/labels, or 2 MiB aggregate schematic text before cloning. It captures only sheets, never board data. Native content is validated, including exact transforms and unique identities. Calculation runs outside the engine lock. A wire bounding-box spatial index narrows point contacts; exact integer predicates refine candidates. At most 1000000 candidate contacts are tested, including bounding-box false positives. Dense geometry exceeding this budget fails with an actionable error and no partial result. These limits bound work and output; there is no full-project connectivity/ERC or performance claim for excluded structures.

## Verification

Fixtures must distinguish crossing from junction and T contacts, diagonal near misses at 1 nm, collinear overlap, pin interior vs endpoint, rotated/mirrored pin endpoints, sheet-local vs global/power scope, case sensitivity and aliases. Invalid legacy/extreme content, ambiguous labels, object/candidate bounds, pagination, repeated deterministic output and unchanged session/history are checked. Real two-client HTTP MCP discovery and edit/query/undo flows verify shared engine behavior.
