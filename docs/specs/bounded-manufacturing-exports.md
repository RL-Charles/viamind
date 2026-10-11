# Bounded manufacturing snapshots and exports

## Public sources

- [RFC4180 section2](https://www.rfc-editor.org/rfc/rfc4180): CSV record boundaries, quoting comma/quote/newline fields and doubling embedded quotes. Existing generic Viamind field semantics remain those in assembly-csv.md.
- [Rust AtomicUsize](https://doc.rust-lang.org/std/sync/atomic/struct.AtomicUsize.html): atomic compare/update permits bounded admission across concurrent callers without waiting under the engine mutex.
- [Tokio spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html): started blocking operations can outlive cancellation of the awaiting task; admission must remain owned by the blocking operation.
- [Ucamco Gerber2026.05](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf) and [XNC2021.11](https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf): existing file geometry/coordinate conventions remain as specified in fabrication-gerber.md. Public publisher downloads could not be fetched during this increment; no new format interpretation is introduced.

## Behavior

1. Assembly capture copies only component ID, reference, value, footprint, integer origin, stored rotation and side. Pads, schematic links/content, copper and outlines are excluded. Fabrication capture copies project identity/name and supported board geometry into an empty schematic shell. Unrelated sheet contents and IDs never affect or enter either snapshot.
2. Check the requested cursor, bounded counts, individual text and aggregate text before cloning. Capture source cursor and active-native path in the same engine critical section. Never copy the full session/project for manufacturing export. Stale/budget/busy failures leave design/history/status unchanged and publish no output.
3. Assembly supports at most100000 components,4096 UTF-8 bytes per reference/value/footprint,2MiB aggregate copied text and32MiB CSV output. Fabrication supports at most200000 budgeted objects/primitives (including part headers, pads/mask copies, tracks, via graphics and outline vertices),4096 bytes per copied string,2MiB aggregate board/project text and existing128MiB package output. Validate budgets before clone/render. Counts include empty components and outline rings to bound otherwise empty collections.
4. At most two manufacturing snapshots may exist per process, shared across direct library, CLI, desktop and MCP. Admission is nonblocking and fails with a readable busy error. A private RAII permit is acquired before cloning, retained through render/publication and released on success/failure/drop. A canceled awaiting MCP client cannot release a permit held by its blocking job. No awaiting while the engine mutex is held.
5. Public render/write entry points use the same bounded captures; renderer snapshots are immutable and cannot be constructed with unchecked contents. CSV rendering returns a Result to expose budget failures. Accepted existing fixtures preserve all bytes, ordering, grouping, units and escaping. Gerber/XNC/manifest bytes and supported geometry remain unchanged.
6. Preserve CSV explicit-overwrite behavior, protected active-native paths and atomic NEW fabrication-directory publication. Temporary work is not published on validation errors. A failed concurrent request must not alter accepted outputs.
7. Desktop fabrication obtains a cheap cursor without full-session cloning, uses the bounded shared snapshot on its existing worker and reports busy/validation errors. No new manufacturer presets, sourcing identifiers, layers, model commands or fabrication-readiness claims.

## Verification

Compare accepted fixture CSV/Gerber/XNC/manifest bytes to prior output; independently parse escaped Unicode/quotes/newlines CSV. Inspect snapshot exclusions including invalid and large unrelated sheets/pads; test count/individual/aggregate boundaries, admission release after error/drop, malformed direct board models and failed publication preservation. Test stale guarded MCP captures and canceled worker lifetime, desktop parity and all Linux workspace gates.
