//! Atomic property edits, exact history, and stale snapshot protection.
use serde_json::json;
use viamind_doc::{Project, Side, file};
use viamind_engine::{EngineHandle, Origin};
use viamind_geom::Nm;

fn add(engine: &EngineHandle, name: &str) -> viamind_doc::ObjectId {
    engine
        .execute("add_part", json!({ "designator": name, "value": "10k", "footprint": "R_0603", "x_mm": 1, "y_mm": 2 }), Origin::User)
        .unwrap()
        .created[0]
}
#[test]
fn properties_apply_as_one_exact_undoable_saved_change() {
    let engine = EngineHandle::new(Project::new("Properties"));
    let id = add(&engine, "R1");
    let before = engine.read(Clone::clone);
    let original = before.board.part(&id).unwrap();
    let changes = engine.subscribe();
    let result = engine.execute("update_part", json!({ "id": id, "expected": original, "designator": " C9 ", "value": " 100nF ", "footprint": " C_0805 ", "x_mm": 1.234567, "y_mm": -9.876543, "rotation_deg": -90.125, "side": "bottom" }), Origin::agent("editor")).unwrap();
    assert_eq!(result.deltas.len(), 1);
    assert!(result.created.is_empty());
    let after = engine.read(Clone::clone);
    let part = after.board.part(&id).unwrap();
    assert_eq!(part.designator, "C9");
    assert_eq!(part.value, "100nF");
    assert_eq!(part.footprint, "C_0805");
    assert_eq!(part.position.x, Nm(1_234_567));
    assert_eq!(part.position.y, Nm(-9_876_543));
    assert_eq!(part.rotation_mdeg, 269_875);
    assert_eq!(part.side, Side::Bottom);
    assert_eq!(changes.try_recv().unwrap().origin, Origin::agent("editor"));
    assert_eq!(file::from_str(&file::to_string(&after)).unwrap(), after);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
}
#[test]
fn invalid_conflicting_and_stale_edits_leave_model_history_and_events_unchanged() {
    let engine = EngineHandle::new(Project::new("Validation"));
    let id = add(&engine, "R1");
    add(&engine, "R2");
    let stale = engine.read(|p| p.board.part(&id).unwrap().clone());
    engine.execute("update_part", json!({ "id": id, "value": "new value" }), Origin::agent("other")).unwrap();
    let before = engine.session_state();
    let history = engine.with_engine(|e| e.undo_history().len());
    let changes = engine.subscribe();
    for input in [
        json!({ "id": id, "expected": stale, "value": "lost edit" }),
        json!({ "id": id, "designator": "R2", "value": "lost edit" }),
        json!({ "id": id, "designator": " " }),
        json!({ "id": id, "footprint": " " }),
        json!({ "id": id, "value": "lost edit", "x_mm": 10001 }),
        json!({ "id": id, "rotation_deg": 1000001 }),
        json!({ "id": id, "side": "front" }),
        json!({ "id": id, "typo": 3 }),
        json!({ "id": viamind_doc::ObjectId::new(), "value": "missing" }),
    ] {
        assert!(engine.execute("update_part", input, Origin::User).is_err());
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.with_engine(|e| e.undo_history().len()), history);
    assert!(changes.try_recv().is_err());
    let no_change = engine.execute("update_part", json!({ "id": id }), Origin::User).unwrap();
    assert!(no_change.deltas.is_empty());
    assert_eq!(engine.session_state(), before);
    assert!(changes.try_recv().is_err());
}
#[test]
fn omitted_coordinates_keep_exact_model_values_and_placement_rejects_a_different_project() {
    let mut project = Project::new("Large exact coordinates");
    let engine = EngineHandle::new(project.clone());
    let id = add(&engine, "R1");
    project = engine.read(Clone::clone);
    project.board.parts.get_mut(&id).unwrap().position.x = Nm(i64::MAX - 1);
    let engine = EngineHandle::new(project.clone());
    engine.execute("update_part", json!({ "id": id, "value": "exact" }), Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().position.x), Nm(i64::MAX - 1));
    let before = engine.session_state();
    assert!(
        engine
            .execute(
                "add_part",
                json!({ "expected_project": viamind_doc::ObjectId::new(), "designator": "R2", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 }),
                Origin::User
            )
            .is_err()
    );
    assert_eq!(engine.session_state(), before);
}
