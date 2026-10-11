//! Document replacement keeps shared clients and validates every discard.
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use viamind_doc::{Project, file};
use viamind_engine::{ChangeKind, EngineHandle, NewProject, OpenProject, Origin, SessionError, SetSelection, WatchEvents};

fn add(engine: &EngineHandle, name: &str) {
    engine.execute("add_part", json!({ "designator": name, "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 }), Origin::User).unwrap();
}

#[test]
fn new_and_open_reset_documents_but_preserve_shared_clients_and_listeners() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("loaded.viamind.json");
    let loaded = Project::new("Loaded");
    file::save_new(&loaded, &path).unwrap();
    let engine = EngineHandle::new(Project::new("Original"));
    let client = engine.clone();
    add(&engine, "R1");
    let id = engine.read(|p| *p.board.parts.keys().next().unwrap());
    engine.set_selection(SetSelection { ids: vec![id], mode: viamind_engine::SelectionMode::Replace }, Origin::User).unwrap();
    let before = engine.session_state();
    let changes = engine.subscribe();
    let called = Arc::new(AtomicUsize::new(0));
    let count = called.clone();
    let callback = engine.clone();
    engine.on_change(move || {
        assert!(!callback.document_status().dirty);
        count.fetch_add(1, Ordering::SeqCst);
    });
    let opened = client
        .open_project(OpenProject { path: path.clone(), discard_dirty: true, expected_cursor: Some(before.cursor) }, Origin::agent("opener"))
        .unwrap();
    assert_eq!(opened.project, loaded);
    assert!(opened.selection.ids.is_empty());
    assert_ne!(opened.cursor.stream_id, before.cursor.stream_id);
    assert_eq!(opened.cursor.sequence, 0);
    assert_eq!(engine.read(Clone::clone), loaded);
    assert_eq!(engine.document_status().path, Some(path));
    assert!(!engine.can_undo());
    assert!(!engine.can_redo());
    assert_eq!(engine.with_engine(|e| e.log().replay()), loaded);
    assert_eq!(changes.try_recv().unwrap().kind, ChangeKind::Replaced);
    assert_eq!(called.load(Ordering::SeqCst), 1);
    assert_eq!(engine.watch_events(WatchEvents { after: Some(before.cursor), ..Default::default() }), Err(SessionError::WrongSession));
    client.new_project(Default::default(), Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.name.clone()), "Untitled");
    assert_eq!(engine.document_status().path, None);
    assert_eq!(changes.try_recv().unwrap().kind, ChangeKind::Replaced);
    assert_eq!(called.load(Ordering::SeqCst), 2);
}

#[test]
fn unsafe_stale_and_failed_replacements_preserve_everything() {
    let engine = EngineHandle::new(Project::new("Keep"));
    add(&engine, "R1");
    let old = engine.session_state().cursor;
    add(&engine, "R2");
    engine.undo(Origin::User).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let changes = engine.subscribe();
    for input in [
        NewProject::default(),
        NewProject { discard_dirty: true, ..Default::default() },
        NewProject { discard_dirty: true, expected_cursor: Some(old), ..Default::default() },
        NewProject { name: " ".into(), discard_dirty: true, expected_cursor: Some(before.cursor) },
    ] {
        assert!(engine.new_project(input, Origin::User).is_err());
    }
    let directory = tempfile::tempdir().unwrap();
    let invalid = directory.path().join("invalid.json");
    std::fs::write(&invalid, "not json").unwrap();
    for path in [invalid, directory.path().join("missing.json"), "".into()] {
        assert!(engine.open_project(OpenProject { path, discard_dirty: true, expected_cursor: Some(before.cursor) }, Origin::User).is_err());
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert!(engine.can_redo());
    assert!(changes.try_recv().is_err());
    engine.new_project(NewProject { discard_dirty: true, expected_cursor: Some(before.cursor), name: "Replacement".into() }, Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.name.clone()), "Replacement");
}
