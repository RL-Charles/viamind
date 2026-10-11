//! Outline edits share atomic history, dirty state and replay.
use serde_json::{Value, json};
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, Origin};
fn outline() -> Value {
    json!({ "outer": [{"x_mm":0,"y_mm":0},{"x_mm":40,"y_mm":0},{"x_mm":40,"y_mm":30},{"x_mm":0,"y_mm":30}], "cutouts": [[{"x_mm":10,"y_mm":10},{"x_mm":20,"y_mm":10},{"x_mm":20,"y_mm":20},{"x_mm":10,"y_mm":20}]] })
}
#[test]
fn outline_changes_are_atomic_undoable_and_replayable() {
    let engine = EngineHandle::new(Project::new("Outline"));
    let before = engine.read(Clone::clone);
    let result = engine
        .execute("set_board_outline", json!({"outline":outline(),"expected":{"project_id":before.id,"outline":null}}), Origin::agent("outline"))
        .unwrap();
    assert_eq!(result.deltas.len(), 1);
    assert!(engine.document_status().dirty);
    let after = engine.read(Clone::clone);
    assert_eq!(engine.with_engine(|e| e.log().replay()), after);
    assert_eq!(file::from_str(&file::to_string(&after)).unwrap(), after);
    let mut log = Vec::new();
    viamind_engine::EventLog::new(after.clone()).write_jsonl(&mut log).unwrap();
    assert!(String::from_utf8_lossy(&log).contains("\"version\":7"));
    assert_eq!(viamind_engine::EventLog::read_jsonl(log.as_slice()).unwrap().replay(), after);
    let mislabeled = String::from_utf8(log).unwrap().replace("\"version\":7", "\"version\":1");
    assert!(viamind_engine::EventLog::read_jsonl(mislabeled.as_bytes()).is_err());
    let snapshot = engine.session_state();
    for input in [
        json!({}),
        json!({"outline":null,"expected":{"project_id":before.id,"outline":null}}),
        json!({"outline":{"outer":[]}}),
        json!({"outline":{"outer":[{"x_mm":10001,"y_mm":0}]}}),
    ] {
        assert!(engine.execute("set_board_outline", input, Origin::User).is_err());
        assert_eq!(engine.session_state(), snapshot);
    }
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    let unchanged = engine.execute("set_board_outline", json!({"outline":outline()}), Origin::User).unwrap();
    assert!(unchanged.deltas.is_empty());
    engine.execute("set_board_outline", json!({"outline":null}), Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
}
