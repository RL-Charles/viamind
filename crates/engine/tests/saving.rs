//! Saved baselines survive undo, selection, I/O failure and reload.
use serde_json::json;
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, Origin, SaveProject, SelectionMode, SetSelection, WatchEvents};

fn add(engine: &EngineHandle, name: &str) {
    engine.execute("add_part", json!({ "designator": name, "value": "10k", "footprint": "R_0603", "x_mm": 0, "y_mm": 0 }), Origin::User).unwrap();
}

#[test]
fn saved_baseline_and_active_path_are_shared_and_undo_restores_clean_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("power board.viamind.json");
    let engine = EngineHandle::new(Project::new("save"));
    assert!(!engine.document_status().dirty);
    assert!(engine.save_project(Default::default()).unwrap_err().to_string().contains("supply path"));
    add(&engine, "R1");
    assert!(engine.document_status().dirty);
    let before_save = engine.session_state();
    let documents = engine.subscribe();
    let result = engine.save_project(SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    assert_eq!(result.saved_cursor, before_save.cursor);
    assert!(!result.dirty);
    assert_eq!(engine.document_status().path, Some(path.clone()));
    assert_eq!(file::load(&path).unwrap(), before_save.project);
    assert!(documents.try_recv().is_err());
    assert!(engine.watch_events(WatchEvents { after: Some(before_save.cursor), ..Default::default() }).unwrap().events.is_empty());
    assert_eq!(engine.with_engine(|e| e.undo_history().len()), 1);
    engine.undo(Origin::User).unwrap();
    assert!(engine.document_status().dirty);
    engine.redo(Origin::User).unwrap();
    assert!(!engine.document_status().dirty);
    let id = engine.read(|project| *project.board.parts.keys().next().unwrap());
    engine.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Replace }, Origin::User).unwrap();
    assert!(!engine.document_status().dirty);
    add(&engine, "R2");
    engine.save_project(Default::default()).unwrap();
    assert_eq!(file::load(&path).unwrap().board.parts.len(), 2, "omitted path replaces the active file");
    let reloaded = EngineHandle::load(&path).unwrap();
    assert_eq!(reloaded.document_status(), engine.document_status());
    assert_eq!(reloaded.read(Clone::clone), engine.read(Clone::clone));
    assert!(!reloaded.can_undo());
}

#[test]
fn failed_saves_leave_files_metadata_and_redo_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.viamind.json");
    let engine = EngineHandle::new(Project::new("save"));
    engine.save_project(SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    let original = std::fs::read(&path).unwrap();
    add(&engine, "R1");
    add(&engine, "R2");
    engine.undo(Origin::User).unwrap();
    let before = engine.document_status();
    for input in [
        SaveProject { path: Some(path.clone()), overwrite: false },
        SaveProject { path: Some(directory.path().join("missing/file.json")), overwrite: false },
        SaveProject { path: Some("".into()), overwrite: false },
    ] {
        assert!(engine.save_project(input).is_err());
        assert_eq!(engine.document_status(), before);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(engine.can_redo());
    }
    engine.save_project(SaveProject { path: Some(path.clone()), overwrite: true }).unwrap();
    assert!(!engine.document_status().dirty);
    assert!(engine.can_redo(), "saving preserves redo");
    assert_eq!(file::load(path).unwrap().board.parts.len(), 1);
}
