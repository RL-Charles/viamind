# T124 verification coverage and boundaries

The baseline and five original compact tests establish the narrow production defect
and unchanged existing reachability. Two follow-up compact tests strengthen evidence
for effective text-input clips and actual electrical-popup ownership; their result
will be recorded separately from the original 983-test product/native source.

| Requirement | Actual evidence | Boundary |
| --- | --- | --- |
| Fourteen entry buttons, three draft actions | `schematic/sidebar_tests.rs`: baseline reveal/action tests; original and combined native captures | Full painted effective clips at640×400; reveal is distinct from activating every library dialog |
| Sidebar wheel, local session/status/camera | Actual wheel test and every-pass native invariants | Disclosed egui/native input-path events; no OS-input claim |
| Exact local text/caret after row removal | Retained pin field test | Explicit focused field ID, text and cursor range |
| All supported text-field input clips | New `compact_sidebar_all_text_fields_are_clipped_reachable_and_cancel_remains_local` | All symbol metadata/body/coordinate/pin text fields, both wire endpoints, label name; no max-pin stress claim |
| Electrical popup ownership | New `compact_sidebar_electrical_popup_ownership_survives_preceding_row_removal_without_reuse` | Actual selection before/after removal, owner widget IDs and fresh-row no-reuse; outside click may close menu |
| Apply/cancel/keyboard undo | Compact local-draft/Apply test and real20-check desktop MCP report | MCP proves shared commands; local drafts/caret are tested separately |
| Sheet creation/name, selected symbol edit/delete | `tests/schematic.rs::actual_forms_create_all_schematic_types_edit_and_undo_without_board_changes` | Inherited harness1600×1800, functional actions; not a new effective-clip audit |
| Stale sheet/object reload | `tests/schematic.rs::invalid_and_stale_drafts_preserve_every_session_field_until_explicit_reload` | Inherited harness1600×1800; invalid precision, peer rename, stale Apply/rename, explicit reload |
| Same-project reopen | `tests/schematic.rs::reopen_same_project_resets_old_draft_using_session_stream` | Inherited stream-reset/draft cancellation behavior |
| Wire Finish/undo | `tests/schematic_wires.rs::actual_pointer_exact_rotated_mirrored_offgrid_attachment_finish_and_undo` | Inherited1600×2000 canvas input and Enter finish, not a compact Finish-button clip assertion |
| Wire cancel/pan/zoom and stale Finish | `actual_posture_backspace_pan_zoom_and_escape_keep_entire_model_unchanged`, `actual_stale_peer_edit_reopen_and_object_draft_guards_preserve_content` | Inherited1600×2000 Escape cancellation and stale Finish-button rejection |
| Fit/pick/low zoom | `tests/schematic_labels.rs` three actual tests and `tests/schematic.rs::view_fit_and_status_follow_the_visible_schematic_camera` | Original viewport/detail policy remains; no new label or picking geometry |
| Existing limits/virtualized object list | Existing bounded schematic snapshot/engine tests in full workspace | No T124 layout/virtualization change; no new last1024-pin/last-object interaction claim |

The reviewed baseline-informed spec amendment is required before final submission.
Follow-up test results and source are recorded in the final guide/master after the
focused gate. Original/full/native source identities remain unchanged.
