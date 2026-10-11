# Native schematic capture foundation

## Public behavioral sources

KiCad's public [schematic format documentation](https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/)
describes symbols with pin identities, wires, explicit junctions, and local/global
labels. Its [schematic editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html)
describes placed symbols and electrical connection points. These establish the
object categories; Viamind uses its own JSON model and commands. No KiCad file
compatibility, proprietary assets or library symbols are included.

## Native model

- A sheet optionally contains schematic `content`. Legacy sheets omit this field
  and retain their existing serialized bytes. Any explicit content requires native
  project version 5; newly written event logs use version 5.
- Content holds UUID-keyed symbol, wire, junction and label maps. Symbols have
  unique project-wide reference designators, value, optional footprint name,
  integer Nm origin, cardinal quarter-turn rotation, optional local X reflection,
  rectangular body size and ordered local pin endpoints. Pins have stable UUIDs,
  unique numbers within their symbol, optional names and electrical types.
- Coordinates increase right/down and are bounded to +/-10000 mm. Positive body
  dimensions are <=10000 mm. Mirror local X first, then rotate CCW as viewed on
  the sheet. Cardinal transforms use exact integer arithmetic.
- Wires are nonzero straight segments. Junctions are explicit points. Labels are
  named points with local/global/power scope. Electrical graph evaluation, ERC,
  buses, hierarchy, multi-unit symbols and board annotation are follow-up work.
- Names/numbers are trimmed, nonempty, <=128 characters without controls. Values
  may be empty and are bounded to 1024 characters; pin names may be empty and are
  bounded to 128. Each sheet supports <=10000 objects and <=100000 pins, each
  symbol <=1024 pins; project supports <=1000 sheets and <=100000 objects/pins.
  Keys must equal object IDs; new schematic IDs are non-nil and globally distinct across
  schematic and board objects. Unknown model fields reject.

## Shared commands

- `edit_schematic_sheet`: exact project UUID and sheet snapshot guard. Omit both
  sheet ID and expected snapshot to create a new sheet. Supply a name and <=1000
  edits: upsert symbol/wire/junction/label or delete an object. Omitted object IDs
  allocate fresh UUIDs; supplied IDs must refer to the same object kind already
  present. Symbol pin numbers retain identities when editing; new numbers get
  fresh IDs. All operations validate before one sheet delta commits. An invalid
  operation rejects the entire batch; no history/event/dirty changes.
- `delete_schematic_sheet`: exact project and complete sheet guard; one undoable
  deletion including its contents.
- All edits share the engine and connection-specific undo. Sheet deltas give
  conservative same-sheet conflict detection for selective undo.
- `query_schematic` returns UUID-ordered bounded pages of sheet summaries, or
  tagged objects from a chosen sheet, with totals and next offset. Limit 1..100,
  read-only. Use `get_session_state` for exact sheet snapshots before editing;
  restart paginated reads after concurrent changes.

## Verification

Exact cardinal/mirrored pin transforms, global identities and malformed data;
legacy byte stability and version rejection; guarded batch rollback, pin identity
retention, native/log replay and undo; real two-client HTTP discovery/edit/stale
rejection/connection undo. Desktop capture is a separate dependent increment.
