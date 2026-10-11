# Detached native rectangular footprint definition editing

Ticket: T-0098, parent S-035. This behavior spec precedes implementation and
requires independent review. Base: merged main 9a80afc (T94).

## Public behavior and scope

Public primary references, read 2026-10-11:

- https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/ describes a named
  standalone footprint definition, independent of a PCB instance.
- https://dev-docs.kicad.org/en/file-formats/sexpr-intro/ describes local
  containing-object coordinates and explicit pad shape/position/size fields.

Only those documented concepts inform behavior. No external editor source,
footprints, images or assets are copied. The existing Viamind footprint JSON
format/validator and native definition editor patterns are the implementation
foundation. This increments S035; courtyard, silkscreen, paste/mask, holes,
nonrectangular shapes and complete footprint authoring remain open.

## Detached definition and exact entry

Expose a footprint-definition window without requiring a selected PCB part.
Explicitly create an empty local definition or load a bounded native library
file. The draft has an editable name and ordered rectangular SMD rows:
number, local X/Y, width and height. A newly added row has blank editable values
requiring explicit entry. Do not infer pitch, origin/centroid, mounting type,
assembly population, mask/paste or manufacturing defaults.

Coordinates/dimensions use exact decimal mm to integer Nm, at most six fractional
digits, no scientific/nonfinite/excess-precision input or rounding. Native
validation retains unique trimmed nonempty numbers/names up to128 characters,
local coordinates within±10000mm, positive dimensions up to10000mm and at most
1024 pads. Empty named definitions remain explicitly valid as in the native
format. The local definition has no identities, nets, board side or rotation.

Bound name/number fields before copying (UTF8 <=512bytes consistent with the
existing128-character model limit), numeric draft fields<=64bytes and aggregate
draft text<=1MiB. Reject unsupported inputs with actionable errors and retain the
fields; never allocate a1025th row. Use stable editor-only row identities so
removing one row cannot transfer focus/text to another row. Validate borrowed
counts/text before constructing/cloning typed geometry. Existing serialized
1MiB native-file cap remains authoritative.

Show a local rectangular preview with explicit pad numbers and origin/axes.
Fit/selection affect only the view; display local centers/dimensions without
changing geometry. Float conversion is restricted to rendering. Invalid draft
rows remain editable and are not presented as a validated electrical footprint.
No inferred courtyard/body/centroid or fab equivalence is displayed.

## Existing files and shared parity

Add a bounded shared read-only native definition loader and typed NEW-only
writer through EngineHandle/MCP/UiAction. Reuse footprint::load (same-fd
nonblocking regular-file reader), Footprint::validate/to_vec and stage_new /
with_native_file_guard/publish. Nonempty source/output paths and resolved output paths <=4096 encoded bytes;
source and output<=1MiB. Unknown schema fields reject.

The writer accepts {footprint,path}, validates/stages outside the engine lock,
then publishes under the existing latest active-native-file lifecycle guard.
It never replaces a loaded source, existing output, symlink or native project.
Return canonical path/name/pad count. Detached loading/saving does not mutate
Project, shared selection, undo/history, event cursor or dirty state. No source
cursor or file overwrite option is invented for a detached local edit.

Saving does not apply geometry to any part or change already-placed instances.
After Save NEW, explicitly reuse the path through the existing electrical-pad
editor / import_footprint action. Its exact project+part guard and one atomic
undo retain pad IDs/nets by number and preserve component placement, linked
symbol and assembly metadata. New numbers receive fresh IDs and unassigned nets.
No new implicit instance-update behavior or model/schema change is introduced.

## Desktop worker and lifecycle

One retained bounded background worker performs load/save through shared
UiAction. Disable duplicate operations and mutations that would invalidate the
pending definition; retain fields on failure/disconnection/spawn refusal. A
changed load path/generation discards the old completion. Closing the detached
window retains the draft/pending receiver; it never starts a second worker.
Unrelated design edits and document replacement preserve the detached draft.
Save completion can report the published detached file even after design edits;
it cannot enable a stale target-part application. An explicit Reset action discards the local definition draft. Closing the
window retains it. Neither operation changes source/native files or active
design drafts; pending work remains retained until completion.

The form scrolls at640×400, virtualizes bounded rows and keeps Preview/Save/error
controls reachable. Preserve existing route/wire/pad/file dialogs and MCP action
parity. The next new inspection host self-closes after fsynced actual WGPU
Screenshot capture/invariant logging, with accurate UI-pass counter semantics.

## Acceptance evidence

- Public-source spec committed/reviewed before code, new S035 child after merged T94.
- Strict bounded loader/writer tests: invalid fields/counts/precision/duplicates,
  raced FIFO, destination collision/symlink/native lifecycle race; no session or
  prior file mutation and no partial output.
- Actual UI create/edit/add/remove rows with stable focus, odd-Nm asymmetric
  geometry, exact preview/field values; Save NEW then separate guarded application
  preserving identities/nets/placement/link/assembly and one exact undo/redo.
- Compact actual input scroll-to-Save/error bounds, draft preservation through
  peer edits and same-project reopen, one retained worker, cancellation, failed
  spawn/disconnection/hidden completion and duplicate suppression.
- Two real MCP clients exercise shared load/save/application and all relevant
  unchanged-session/native-file contracts; production stdio and owned native
  HTTP; readable actual WGPU native view with source/binary/process hashes.
- All four Linux gates and final combined CI; durable exact-source evidence and
  independent review. No claim that S035's full graphics/manufacturing scope is done.

## Proposed ownership

New engine footprint_definition.rs and UI footprint_definition.rs modules;
additive EngineHandle exports, EngineHost registration/dispatch, shared UiAction
and Libraries menu integration. Existing pads.rs only gains explanatory reuse
text if needed. No model/schema or dependency change is required. Generated MCP
reference is refreshed after root T96 lands, using its documented generator,
so the catalog covers new shared actions.
