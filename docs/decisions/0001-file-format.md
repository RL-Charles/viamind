# 0001. Native file format: pretty-printed JSON with deterministic order

- Status: accepted
- Date: 2026-10-10
- Ticket: T-0014 (story E1-S5)

## Context

A Viamind project must save as text that:

1. **Is deterministic.** Saving an unchanged project gives byte-identical output, so version control shows no noise.
2. **Diffs and merges well.** A small edit touches a few lines; parallel edits rarely conflict.
3. **Is easy for agents.** LLM agents read and write it without a custom parser, and can reuse the command JSON schemas.
4. **Round-trips the model exactly.** All model lengths are integer nanometres, so no float formatting is involved.
5. **Has a mature Rust serde implementation** and no licence concerns.

## Options

Measured with a 1,000-part board serialised through serde (release build, same model; throwaway spike, not committed):

| Format | Size | Serialise / parse | Notes |
| --- | --- | --- | --- |
| **JSON** (`serde_json`, pretty) | 332 KB | 0.45 / 0.51 ms | Universal; agents' native tool format; no comments |
| RON (`ron` 0.12, pretty) | 410 KB | 1.4 / 2.3 ms | Rust-specific syntax; little tooling outside Rust |
| TOML (`toml` 1.1) | 273 KB | 2.0 / 4.6 ms | No `null`, so `Option` fields need special-casing; deep nesting becomes long dotted table headers |
| KDL (`kdl` 6) | n/a | n/a | No serde support; a hand-written mapping for every type |
| S-expressions (custom) | n/a | n/a | No standard; needs our own writer and parser and a spec |

## Decision

Use **pretty-printed JSON** as the native format:

- 2-space indent, LF line endings, one trailing newline, UTF-8.
- A small envelope: `{ "format": "viamind", "version": 1, "project": { … } }`, so the loader can reject foreign files and migrate old versions.
- File name `<name>.viamind.json`; the `.json` suffix gives editors, GitHub and agents syntax support for free.
- **Order comes from the model, never from insertion:** struct fields serialise in declaration order, and every collection is a `BTreeMap` keyed by `ObjectId`, so objects appear sorted by id. No `HashMap` may appear in the model.
- Lengths are integer nanometres (`Nm`), so numbers print exactly; floats are not stored in the model.
- Maps repeat the id inside each object (`"<id>": { "id": "<id>", … }`), so an object read alone is complete; the loader checks that each key equals the object's `id`.

## Consequences

- Determinism is structural and checked by a test (`crates/doc/tests/format_determinism.rs`): the same project built in any insertion order serialises to the same bytes.
- Objects sorted by random UUID means new objects land at scattered positions, not all at the end of a list, so parallel additions on two branches rarely touch the same lines.
- JSON has no comments; design notes belong in the model (e.g. a future `notes` field), not in the file.
- Files are larger than TOML; compression is left to version control and archives.
- Save/load and the byte-identical re-save test are T-0009.

## Polygon outlines (native schema version 2)

Outline-free projects continue to write the version-1 envelope with identical
bytes. Projects with explicit polygon board outlines write version 2 so older
builds reject files whose substrate geometry they cannot preserve. Version 2
adds the optional `board.outline` outer polygon and cutout rings in integer
nanometres. Both native parsing and engine commands validate ring topology.
See [the behavioral contract](../specs/board-outline.md). Event logs with an
outline-bearing base use log version 2; new outline deltas also cause older
readers to reject unsupported geometry explicitly.

## Electrical pads (native schema version 3)

Explicit per-part `pads` lists require version 3. Each pad has a persistent UUID,
number, local integer-nanometre centre and size, and optional named net assignment.
List order follows the editor; unchanged projects retain exactly the same order
and bytes. Pad-free files retain version 1 or 2 as appropriate. Native loading
validates geometry and globally unique pad identities. New event logs always use
version 3, including empty bases: later appended part snapshots can then contain
pads without older readers silently discarding them. Legacy logs remain readable;
pad-bearing snapshots in a mislabeled older log are rejected.
