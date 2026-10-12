//! Certified DXF preview is immutable; guarded Apply is one undoable, replayable outline delta.
use serde_json::json;
use viamind_doc::{Project, dxf::DxfUnits};
use viamind_engine::{ApplyBoardOutlineDxf, DxfOutlinePreview, EngineHandle, ExpectedOutline, Origin, PreviewBoardOutlineDxf, SaveProject};

const ROUNDED: &str = include_str!("../../doc/tests/fixtures/outline-rounded.dxf");

fn preview_text(text: &str) -> PreviewBoardOutlineDxf {
    PreviewBoardOutlineDxf { text: Some(text.into()), path: None, units: DxfUnits::Header, layer: Some("Edge".into()), tolerance_nm: 10_000 }
}
fn apply_from(p: &PreviewBoardOutlineDxf, preview: &DxfOutlinePreview) -> ApplyBoardOutlineDxf {
    ApplyBoardOutlineDxf {
        text: p.text.clone(),
        path: p.path.clone(),
        units: p.units,
        layer: p.layer.clone(),
        tolerance_nm: p.tolerance_nm,
        expected_source_sha256: preview.source_sha256.clone(),
        expected_cursor: preview.expected_cursor,
        expected: preview.expected.clone(),
    }
}

#[test]
fn preview_is_immutable_and_apply_is_one_guarded_undoable_replayable_delta() {
    let directory = tempfile::tempdir().unwrap();
    let engine = EngineHandle::new(Project::new("Curved DXF"));
    engine.execute("add_part", json!({"designator":"R1","value":"1k","footprint":"R_0603","x_mm":20,"y_mm":-20}), Origin::User).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let input = preview_text(ROUNDED);
    let preview = engine.preview_board_outline_dxf(&input).unwrap();
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(engine.preview_board_outline_dxf(&input).unwrap(), preview, "deterministic for identical bytes/options/session");
    assert_eq!(preview.expected, ExpectedOutline { project_id: before.project.id, outline: None });
    assert_eq!(preview.expected_cursor, before.cursor);
    assert!(preview.changes_outline && !preview.report.straight_only);
    assert_eq!(preview.outline.cutouts.len(), 2);
    assert!(preview.report.achieved_bound_nm <= 10_000);
    assert_eq!(preview.report.ignored_by_type.get("TEXT"), Some(&1));
    assert_eq!(preview.report.rows.iter().filter(|r| r.handle.as_deref() == Some("2A")).count(), 8);
    assert_eq!(preview.source_sha256.len(), 64);

    // Wrong digest, stale guards and changed source refuse without mutation.
    let apply = apply_from(&input, &preview);
    let mut wrong = apply.clone();
    wrong.expected_source_sha256 = "0".repeat(64);
    let mut changed = apply.clone();
    changed.text = Some(ROUNDED.replace("1.6", "1.7"));
    let mut uppercase = apply.clone();
    uppercase.expected_source_sha256 = apply.expected_source_sha256.to_uppercase();
    let mut stream = apply.clone();
    stream.expected_cursor.stream_id = viamind_doc::ObjectId::new();
    let mut outline = apply.clone();
    outline.expected.outline = Some(preview.outline.clone());
    let mut project = apply.clone();
    project.expected.project_id = viamind_doc::ObjectId::new();
    let mut tolerance = apply.clone();
    tolerance.tolerance_nm = 9;
    for (name, bad) in [
        ("digest", wrong),
        ("source", changed),
        ("case", uppercase),
        ("stream", stream),
        ("outline", outline),
        ("project", project),
        ("tolerance", tolerance),
    ] {
        assert!(engine.apply_board_outline_dxf(&bad, Origin::agent("dxf")).is_err(), "{name}");
        assert_eq!(engine.session_state(), before, "{name}");
    }

    let applied = engine.apply_board_outline_dxf(&apply, Origin::agent("dxf")).unwrap();
    assert!(applied.changed);
    assert_eq!(applied.result.deltas.len(), 1);
    assert!(matches!(applied.result.deltas[0], viamind_engine::Delta::BoardOutline { .. }));
    let after = engine.session_state();
    assert_eq!(after.project.board.outline.as_ref(), Some(&preview.outline));
    assert_eq!(after.project.board.parts, before.project.board.parts, "other board objects preserved");
    assert_eq!(applied.cursor, after.cursor);
    assert_eq!(engine.with_engine(|e| e.log().replay()), after.project);
    // The consumed preview is stale; a fresh preview of identical geometry applies as a no-op.
    assert!(engine.apply_board_outline_dxf(&apply, Origin::agent("dxf")).is_err());
    let fresh = engine.preview_board_outline_dxf(&input).unwrap();
    assert!(!fresh.changes_outline);
    let no_op = engine.apply_board_outline_dxf(&apply_from(&input, &fresh), Origin::User).unwrap();
    assert!(!no_op.changed && no_op.result.deltas.is_empty());
    assert_eq!(engine.session_state(), after);

    // Native save/reopen and undo/redo use the ordinary polygon delta.
    let native = directory.path().join("curved.viamind.json");
    engine.save_project(SaveProject { path: Some(native.clone()), overwrite: false }).unwrap();
    assert_eq!(viamind_doc::file::load(&native).unwrap(), after.project);
    assert_eq!(engine.undo_own(Origin::agent("other")).unwrap(), None, "a peer cannot undo the creator's apply");
    engine.undo_own(Origin::agent("dxf")).unwrap().unwrap();
    assert_eq!(engine.read(Clone::clone), before.project);
    engine.redo_own(Origin::agent("dxf")).unwrap().unwrap();
    assert_eq!(engine.read(|p| p.board.outline.clone()), Some(preview.outline));
}

#[test]
fn path_sources_bind_actual_bytes_and_peer_edits_or_rewrites_invalidate_apply() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rounded.dxf");
    std::fs::write(&path, ROUNDED).unwrap();
    let engine = EngineHandle::new(Project::new("Path DXF"));
    let input = PreviewBoardOutlineDxf { text: None, path: Some(path.clone()), units: DxfUnits::Header, layer: None, tolerance_nm: 10_000 };
    // All layers selected: the TEXT note is an unsupported selected entity.
    assert!(engine.preview_board_outline_dxf(&input).unwrap_err().to_string().contains("TEXT"));
    let input = PreviewBoardOutlineDxf { layer: Some("Edge".into()), ..input };
    let preview = engine.preview_board_outline_dxf(&input).unwrap();
    assert_eq!(preview.path.as_deref(), Some(path.as_path()));
    assert_eq!(preview.source_sha256, engine.preview_board_outline_dxf(&preview_text(ROUNDED)).unwrap().source_sha256);
    let apply = apply_from(&input, &preview);
    // Source rewrite after preview: digest/reparse rejects.
    std::fs::write(&path, ROUNDED.replace("40\n1.6", "40\n1.5")).unwrap();
    let before = engine.session_state();
    assert!(engine.apply_board_outline_dxf(&apply, Origin::User).unwrap_err().to_string().contains("changed"));
    std::fs::write(&path, ROUNDED).unwrap();
    // Unrelated peer edit after preview: full cursor guard rejects.
    engine.execute("add_part", json!({"designator":"P1","value":"peer","footprint":"X","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    assert!(engine.apply_board_outline_dxf(&apply, Origin::User).is_err());
    assert!(engine.read(|p| p.board.outline.is_none()));
    assert_ne!(engine.session_state(), before);
    let fresh = engine.preview_board_outline_dxf(&input).unwrap();
    engine.apply_board_outline_dxf(&apply_from(&input, &fresh), Origin::User).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), ROUNDED, "source bytes are never written");
    // Exactly one source.
    let both = PreviewBoardOutlineDxf { text: Some(ROUNDED.into()), ..input.clone() };
    let neither = PreviewBoardOutlineDxf { path: None, ..input };
    for bad in [both, neither] {
        assert!(engine.preview_board_outline_dxf(&bad).unwrap_err().to_string().contains("exactly one"));
    }
}

#[test]
fn straight_sources_through_the_new_entry_match_the_legacy_command_exactly() {
    for fixture in [include_str!("../../doc/tests/fixtures/outline-mm.dxf"), include_str!("../../doc/tests/fixtures/outline-lines.dxf")] {
        let legacy = EngineHandle::new(Project::new("legacy"));
        let id = legacy.read(|p| p.id);
        legacy.execute("import_board_outline_dxf", json!({"text":fixture,"expected":{"project_id":id,"outline":null}}), Origin::User).unwrap();
        let curved = EngineHandle::new(Project::new("curved"));
        let input = PreviewBoardOutlineDxf { text: Some(fixture.into()), path: None, units: DxfUnits::Header, layer: None, tolerance_nm: 10 };
        let preview = curved.preview_board_outline_dxf(&input).unwrap();
        curved.apply_board_outline_dxf(&apply_from(&input, &preview), Origin::User).unwrap();
        let (a, b) = (legacy.read(|p| p.board.outline.clone()), curved.read(|p| p.board.outline.clone()));
        assert_eq!(serde_json::to_vec(&a).unwrap(), serde_json::to_vec(&b).unwrap());
    }
}
