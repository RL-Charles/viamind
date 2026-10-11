# Stable detached symbol editor controls and bounded writer requests (T-0102)

## Public behavior and sources

Independent behavior from the public [KiCad 9 symbol editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols), accessed 2026-10-11: detached symbols expose properties and ordered pins with number/name/local coordinates/electrical types and can be saved for reuse. Existing Viamind native single-unit rectangular definitions remain authoritative; no external implementation, symbols or assets are copied.

The pinned [egui 0.36.2 identity documentation](https://docs.rs/egui/0.36.2/egui/struct.Id.html) explains identity must remain unique when stateful controls move; [UiBuilder identity](https://docs.rs/egui/0.36.2/egui/struct.UiBuilder.html#method.id) describes parent-independent identities. Consulted compatible MIT/Apache egui source for this API only. This follow-up fixes control identity, and prevents secondary typed allocation when saving an already-decoded MCP request. It does not add multi-unit parts, graphics, formats or update placed instances.

## Stable actual field identity

Keep each detached editor instance and pin row's lifetime identity independent of row index, label text, automatic layout IDs, virtualization and parent UI position. Explicit metadata controls cover definition name/value/optional footprint/body width/body height/NEW path. Each pin's number/name/X/Y/electrical-type control has a persistent identity tied to that row and field, distinct from every other row/control. Add/remove/scroll preserve surviving rows' actual egui/AccessKit IDs and committed text/electrical value; deleting a focused row must not transfer its editing state to another row. A newly created or explicitly replaced editor gets fresh identities.

Actual keyboard editing/cursor selection remains attached to the same surviving field while unrelated row deletion or virtualization changes layout. Clicking Remove explicitly changes keyboard focus as usual; this spec does not promise a clicked-away field stays focused. Verify continuity of actual field identity and text/cursor state rather than only the internal row UUID. Electrical-type popup state must refer to its original row, never another row after indices change.

Existing local detached draft, busy/Close/Cancel semantics, exact decimal Nm conversion, order, native file bytes, strict validation and shared guarded separate placement are unchanged. A local edit creates no project/history/selection/file change. Use remains explicit; NEW save remains separate from place.

## Borrowed writer boundary

Before `save_symbol_template` converts an already parsed JSON Value into a typed SaveSymbolTemplate, inspect borrowed fields only:

- `template.pins` array count <= 1024, checked before any row visits or typed geometry allocation.
- Known textual definition name/value/optional footprint and each pin number/name <= 4096 UTF-8 bytes per field and <= 262144 aggregate bytes, with checked/saturating accumulation before secondary owned strings/rows.
- Path <= 4096 UTF-8 bytes before PathBuf construction or OS calls.

Traverse at most the bounded 1024 rows and the five known definition/pin string classes. Unsupported/missing/wrongly typed fields continue through strict serde rejection after limit preflight; do not silently coerce or skip invalid typed geometry. Limit errors take precedence over malformed rows:1025 null rows must produce the count error, not a null-row deserialize error. Oversized name/number or aggregate strings paired with malformed geometry must reject the documented budget first. Exactly-boundary valid definitions continue through native semantic validation, which retains stricter name character/trim/unique pin and geometry rules.

JSON transport parsing already materializes Value; this preflight bounds secondary typed allocation and does not claim a new global transport cap. Existing source reader/output file 1 MiB limit and latest-active-native-path guarded NEW publication remain intact. No new model/schema/dependency/tool and no valid serialized template/schema bytes are introduced. This scope hardens the detached writer, not every existing command's unrelated expected-sheet decoding.

## Verification

- Actual editor tests retain all pin field and electrical control AccessKit identities/text/type after deleting an earlier pin, scrolling virtualized rows away/back and changing surrounding metadata layout; removed row identities do not transfer. Check distinctness across fields and fresh editor replacement.
- Actual640×400 create/edit/save controls remain reachable, with unchanged exact odd Nm template file and full SessionSnapshot/document status. Explicit Use/placement still produces fresh instance/pinIDs, separate guarded oneundo.
- Direct/realMCP negative max+1-null-row, perfield max+1, aggregate max/max+1 and path max+1 cases prove budget-first rejection and no output/session change. Invalid nested/wrong types still refuse. Existing valid NEW/collision/native-path protections and canonical tool schemas remain unchanged.
- Linux fmt/strict workspace/all-target Clippy/workspace tests/build, regenerated full application catalog exactcheck, actual two-client/stdio and self-closing readable native evidence with exact source/binary/report hashes. Final combined CI and independent review before merge.

Parent S034 multi-unit quad-op-amp acceptance remains open.
