# Nonplated-hole foundation verification

Ticket T-0121. This is an intermediate model, geometry and persistence phase against
immutable aperture foundation `950f6160cd251072a04a0bf47bc25d24313434fc`.
It is not a supported hole-enabled application or a finished physical-consumer release.

## Contract and source

The public [contract](specs/nonplated-footprint-holes.md) and its foundation amendment
were independently approved before Rust. The amendment's original public commit is
`d106299b83e6cbb056d1fbaea922dfd8f3da91fe`; the own-only transplant is tagged
`verification/t0121/foundation-pre-code-spec`.

Model implementation `b9621579` plus constructor-only correction `b08bee5b` adds strict
bounded circular hole rows, globally checked identities, exact cardinal geometry,
computed arbitrary-angle quantization, optional Part/footprint storage, native15,
library4 and log15 composition. Existing Part copy helpers preserve drills with
`pads:None`. Constructors for existing fixtures and legacy flows explicitly use None.
Hole commands and physical consumers are still pending.

## Results

The [evidence index](verification/t0121/foundation-evidence.json) records exact source,
artifact hashes and scope. The original scope stopped at two test-helper initializer
omissions before tests; its failed log is retained. After that correction, the remaining
scope passed formatting, strict workspace linting with all targets, 24 document checks
and seven engine checks: 31 passed, zero failed.

Nine new hole checks cover absent/null/empty authoring intent and version composition,
strict duplicate/unknown fields, extra poison-row admission, odd half-edge and key/ID
bounds, eight independently prepared cardinal/mirror centers, computed arbitrary-pose
quantization, global collisions, padless physical copies, aggregate admission, library4
presence, both log delta sides and intermediate global collisions. Existing native,
footprint, aperture and artwork checks supply compatibility coverage.

Independent source review provisionally passed `b9621579`; the correction only completes
an existing test helper. The earlier external integer/fabrication oracles and consumer
inventory are retained as preparation artifacts. They do not establish implemented
fabrication or electrical support.

## Remaining acceptance

Final aperture consumer handoff and a fresh interface audit precede hole command/UI,
DRC/routing/continuity, rendering, fabrication and release integration. Actual compact
and wide native editing, initialized multi-client MCP, independent drill parsing and
all full Linux/application checks remain required. Final independent review and
exact-head CI are required before merge; human completion remains separate.
