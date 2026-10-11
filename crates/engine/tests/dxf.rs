//! DXF import shares guarded history, replay, no-op and failure atomicity.
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
const DXF: &str = include_str!("../../doc/tests/fixtures/outline-mm.dxf");
#[test]
fn import_is_guarded_atomic_undoable_and_replayable() {
    let engine = EngineHandle::new(Project::new("DXF"));
    let before = engine.read(Clone::clone);
    let expected = json!({"project_id":before.id,"outline":null});
    let input = json!({"text":DXF,"expected":expected});
    let imported = engine.execute("import_board_outline_dxf", input.clone(), Origin::agent("dxf")).unwrap();
    assert_eq!(imported.deltas.len(), 1);
    let after = engine.read(Clone::clone);
    assert_eq!(after.board.outline.as_ref().unwrap().cutouts.len(), 1);
    assert_eq!(engine.with_engine(|e| e.log().replay()), after);
    assert_eq!(viamind_doc::file::from_str(&viamind_doc::file::to_string(&after)).unwrap(), after);
    let snapshot = engine.session_state();
    for invalid in [
        input,
        json!({"text":DXF}),
        json!({"text":DXF.replace("LWPOLYLINE","CIRCLE"),"expected":{"project_id":after.id,"outline":after.board.outline}}),
    ] {
        assert!(engine.execute("import_board_outline_dxf", invalid, Origin::User).is_err());
        assert_eq!(engine.session_state(), snapshot);
    }
    let no_op = engine
        .execute("import_board_outline_dxf", json!({"text":DXF,"expected":{"project_id":after.id,"outline":after.board.outline}}), Origin::User)
        .unwrap();
    assert!(no_op.deltas.is_empty());
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
}
