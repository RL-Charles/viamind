use serde_json::json;
use viamind_doc::{
    ObjectId, Project, Sheet, file,
    schematic::{Coordinate, SchematicContent, SchematicWire},
};
use viamind_engine::{Command, CommandCall, DrawSchematicWirePolyline, EngineHandle, EventLog, Origin, SchematicWireSheetQuery};
use viamind_geom::Nm;
fn p(x: i64, y: i64) -> Coordinate {
    Coordinate { x: Nm(x), y: Nm(y) }
}
fn fixture() -> EngineHandle {
    let mut project = Project::new("wire test");
    let mut sheet = Sheet::new("wire sheet");
    sheet.content = Some(SchematicContent::default());
    project.schematic.sheets.insert(sheet.id, sheet);
    EngineHandle::new(project)
}
fn command(e: &EngineHandle, vertices: Vec<Coordinate>) -> DrawSchematicWirePolyline {
    let sheet_id = e.read(|p| *p.schematic.sheets.keys().next().unwrap());
    let snap = e.snapshot_schematic_wire_sheet(&SchematicWireSheetQuery { sheet_id }).unwrap();
    DrawSchematicWirePolyline { expected_project: snap.expected_project, source_cursor: snap.source_cursor, expected: snap.expected, vertices }
}
#[test]
fn exact_atomic_typed_dynamic_undo_native_log_and_immutable_snapshot() {
    for typed in [true, false] {
        let e = fixture();
        let before = e.session_state();
        let cmd = command(&e, vec![p(-1, 3), p(4, 3), p(4, 9)]);
        assert_eq!(before, e.session_state());
        let result = if typed {
            e.execute_typed(cmd, Origin::User)
        } else {
            e.execute(DrawSchematicWirePolyline::NAME, serde_json::to_value(cmd).unwrap(), Origin::User)
        }
        .unwrap();
        let after = e.session_state();
        assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
        assert!(result.created.is_empty(), "existing sheet is modified, nested wire IDs belong to its delta");
        let sheet = after.project.schematic.sheets.values().next().unwrap();
        let wires = &sheet.content.as_ref().unwrap().wires;
        assert!(wires.values().any(|w| w.start == p(-1, 3) && w.end == p(4, 3)));
        assert_eq!(after.project.board, before.project.board);
        assert_eq!(file::from_str(&file::to_string(&after.project)).unwrap(), after.project);
        let mut bytes = vec![];
        e.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
        assert_eq!(EventLog::read_jsonl(&bytes[..]).unwrap().replay(), after.project);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after.project);
    }
}
#[test]
fn crossing_stays_separate_and_t_endpoint_joins_without_implicit_junctions() {
    let e = fixture();
    e.execute_typed(command(&e, vec![p(0, 0), p(10, 0)]), Origin::User).unwrap();
    e.execute_typed(command(&e, vec![p(5, -5), p(5, 5)]), Origin::User).unwrap();
    let query = viamind_engine::SchematicConnectivityQuery { offset: 0, limit: 100 };
    assert_eq!(e.query_schematic_connectivity(&query).unwrap().nets.len(), 2);
    e.execute_typed(command(&e, vec![p(7, -5), p(7, 0)]), Origin::User).unwrap();
    let graph = e.query_schematic_connectivity(&query).unwrap();
    assert_eq!(graph.nets.len(), 2);
    assert_eq!(graph.nets.iter().map(|n| n.members.len()).max(), Some(2));
    assert!(e.read(|p| p.schematic.sheets.values().all(|s| s.content.as_ref().unwrap().junctions.is_empty())));
}
#[test]
fn invalid_stale_forged_reopen_and_batch_leave_whole_session_unchanged() {
    for path in
        [vec![], vec![p(0, 0)], vec![p(0, 0), p(0, 0)], vec![p(0, 0), p(1, 1)], vec![p(0, 0), p(i64::MIN, 0)], (0..257).map(|x| p(x, 0)).collect()]
    {
        let e = fixture();
        let before = e.session_state();
        let cmd = command(&e, path);
        assert!(e.execute_typed(cmd, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
    }
    let e = fixture();
    let old = command(&e, vec![p(0, 0), p(1, 0)]);
    e.execute("add_part", json!({"designator":"R1","value":"peer","footprint":"x","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(old, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    let mut wrong = command(&e, vec![p(0, 0), p(1, 0)]);
    wrong.expected_project = ObjectId::new();
    assert!(e.execute_typed(wrong, Origin::User).is_err());
    let mut wrong = command(&e, vec![p(0, 0), p(1, 0)]);
    wrong.expected.name = "forged".into();
    assert!(e.execute_typed(wrong, Origin::User).is_err());
    let fresh = command(&e, vec![p(0, 0), p(1, 0)]);
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: DrawSchematicWirePolyline::NAME.into(), input: serde_json::to_value(&fresh).unwrap() },
                CommandCall { name: "missing".into(), input: json!({}) }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(fresh, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
#[test]
fn result_and_snapshot_budgets_reject_before_copy_or_commit() {
    let e = fixture();
    let mut project = e.read(Clone::clone);
    let sheet = project.schematic.sheets.values_mut().next().unwrap();
    let sheet_id = sheet.id;
    for n in 0..9999 {
        let id = ObjectId::new();
        sheet.content.as_mut().unwrap().wires.insert(id, SchematicWire { id, start: p(n, 0), end: p(n, 1) });
    }
    let e = EngineHandle::new(project);
    let cmd = command(&e, vec![p(0, 5), p(1, 5), p(1, 6)]);
    let before = e.session_state();
    assert!(e.execute_typed(cmd, Origin::User).is_err());
    assert_eq!(before, e.session_state());
    let mut project = before.project;
    project.schematic.sheets.get_mut(&sheet_id).unwrap().name = "x".repeat(4097);
    let e = EngineHandle::new(project);
    let before = e.session_state();
    assert!(e.snapshot_schematic_wire_sheet(&SchematicWireSheetQuery { sheet_id }).is_err());
    assert_eq!(before, e.session_state());
}
