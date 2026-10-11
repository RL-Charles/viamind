# Part placement and properties

Public behavior reference: the [KiCad PCB editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)
describes footprint properties including reference/value, position, orientation and board side.
Viamind implements independent forms and command semantics below; no external EDA code is used.

- Desktop Place Part opens a form for designator, value, footprint name, X/Y in mm, rotation in
  degrees and top/bottom side. Submit uses add_part with the expected_project ID captured when
  opening the form, so switching to a different document rejects stale placement. Cancel changes nothing.
- The selected-part properties panel shows a single part. Editing uses a local draft and an
  explicit Apply action. Reload discards the draft and reads the latest shared part. A change of
  selection/document replaces the draft; agent edits to the same part leave the draft reviewable.
- `update_part` accepts id and optional designator, value, footprint, x_mm, y_mm, rotation_deg,
  side and expected. Omitted fields keep their current values. Expected is the exact model part
  snapshot (including nanometre position) and rejects if it no longer matches. Desktop Apply
  always includes it. This protects agent edits from a stale properties form.
- Match existing placement validation: nonempty unique trimmed designator, nonempty trimmed
  footprint, finite bounded coordinates, normalized rotation with millidegree resolution.
  Validate all fields before applying one delta; unchanged updates create no history/event.
  Full changes undo/redo exactly, preserve object identity, emit attributed events and save normally.
- Selected-part Delete uses the shared delete_part action, prunes selection and is undoable.
  Existing move_part also enters the action catalog. Full footprint libraries, graphic geometry,
  routing and live ratsnest are separate increments.
