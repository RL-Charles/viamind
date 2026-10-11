use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use viamind_doc::{ObjectId, Project};
use viamind_engine::{EngineHandle, Origin, SelectionMode, SetSelection};

fn add(engine: &EngineHandle, designator: &str) -> ObjectId {
    engine
        .execute("add_part", json!({ "designator": designator, "value": "10k", "footprint": "R_0603", "x_mm": 20, "y_mm": 20 }), Origin::User)
        .unwrap()
        .created[0]
}

#[test]
fn selection_is_shared_atomic_and_preserves_document_history() {
    let engine = EngineHandle::new(Project::new("selection"));
    let a = add(&engine, "R1");
    let b = add(&engine, "R2");
    let duplicate = add(&engine, "R3");
    engine.undo(Origin::User).unwrap();
    let project = engine.read(Clone::clone);
    let log_len = engine.with_engine(|e| e.log().entries().len());
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    let reentrant = engine.clone();
    engine.on_change(move || {
        let _ = reentrant.selection();
        count.fetch_add(1, Ordering::SeqCst);
    });
    let request = |ids, mode| SetSelection { ids, mode };
    let agent = engine.clone();
    std::thread::spawn(move || agent.set_selection(request(vec![a, a], SelectionMode::Replace), Origin::agent("selector"))).join().unwrap().unwrap();
    assert_eq!(engine.selection().ids, [a]);
    assert_eq!(engine.selection().origin, Some(Origin::agent("selector")));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    engine.set_selection(request(vec![a], SelectionMode::Add), Origin::User).unwrap();
    assert_eq!(wakes.load(Ordering::SeqCst), 1, "no-op does not wake");
    let before = engine.selection();
    assert!(engine.set_selection(request(vec![b, duplicate], SelectionMode::Add), Origin::User).is_err());
    assert_eq!(engine.selection(), before, "invalid IDs reject whole selection update");
    engine.set_selection(request(vec![a, b, b], SelectionMode::Toggle), Origin::User).unwrap();
    assert_eq!(engine.selection().ids, [b]);
    engine.set_selection(request(vec![a], SelectionMode::Add), Origin::User).unwrap();
    engine.set_selection(request(vec![b], SelectionMode::Remove), Origin::User).unwrap();
    assert_eq!(engine.selection().ids, [a]);
    assert_eq!(engine.read(Clone::clone), project);
    assert_eq!(engine.with_engine(|e| e.log().entries().len()), log_len);
    assert!(engine.can_redo(), "selection preserves redo");
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.selection().ids, [a]);
}

#[test]
fn deleted_and_undone_parts_leave_selection_and_redo_does_not_restore_it() {
    let engine = EngineHandle::new(Project::new("prune"));
    let a = add(&engine, "R1");
    engine.set_selection(SetSelection { ids: vec![a], mode: SelectionMode::Replace }, Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    assert!(engine.selection().ids.is_empty());
    assert_eq!(engine.selection().revision, 2);
    engine.redo(Origin::User).unwrap();
    assert!(engine.selection().ids.is_empty());
    engine.set_selection(SetSelection { ids: vec![a], mode: SelectionMode::Replace }, Origin::User).unwrap();
    engine.execute("delete_part", json!({ "id": a }), Origin::agent("deleter")).unwrap();
    assert!(engine.selection().ids.is_empty());
    assert_eq!(engine.selection().origin, Some(Origin::agent("deleter")));
}

#[test]
fn mixed_part_track_via_selection_is_atomic_transient_and_pruned_on_deletion() {
    let engine = EngineHandle::new(Project::new("mixed selection"));
    let part = add(&engine, "R1");
    let track = engine
        .execute("add_track", json!({"start_x_mm":0,"start_y_mm":0,"end_x_mm":1,"end_y_mm":0,"width_mm":0.2,"layer":"top"}), Origin::User)
        .unwrap()
        .created[0];
    let via = engine.execute("add_via", json!({"x_mm":1,"y_mm":0,"diameter_mm":0.6,"drill_mm":0.3}), Origin::User).unwrap().created[0];
    let before = engine.read(Clone::clone);
    engine.set_selection(SetSelection { ids: vec![part, track, via], mode: SelectionMode::Replace }, Origin::User).unwrap();
    assert_eq!(engine.selection().ids.len(), 3);
    assert_eq!(engine.read(Clone::clone), before);
    engine.execute("move_part", json!({"id":part,"x_mm":30,"y_mm":30}), Origin::User).unwrap();
    assert_eq!(engine.selection().ids.len(), 3, "unrelated edit preserves selected copper IDs");
    let snapshot = engine.selection();
    assert!(engine.set_selection(SetSelection { ids: vec![track, ObjectId::new()], mode: SelectionMode::Replace }, Origin::User).is_err());
    assert_eq!(engine.selection(), snapshot);
    let expected = engine.read(|p| p.board.tracks[&track].clone());
    engine.execute("delete_track", json!({"id":track,"expected":expected}), Origin::User).unwrap();
    assert_eq!(engine.selection().ids.len(), 2);
    assert!(!engine.selection().ids.contains(&track));
    engine.undo(Origin::User).unwrap();
    assert!(!engine.selection().ids.contains(&track), "undo does not restore transient selection");
    let expected = engine.read(|p| p.board.vias[&via].clone());
    engine.execute("delete_via", json!({"id":via,"expected":expected}), Origin::User).unwrap();
    assert_eq!(engine.selection().ids, vec![part]);
}
