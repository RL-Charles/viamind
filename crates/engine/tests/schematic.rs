use serde_json::{Value, json};
use viamind_doc::{ObjectId, Project, file};
use viamind_engine::{EngineHandle, EventLog, Origin, SchematicQuery};
fn symbol() -> Value {
    json!({"kind":"symbol","symbol":{"designator":"U1","value":"Regulator","footprint":"SMD3","position":{"x":12000001,"y":20000002},"rotation_quarters":1,"mirrored":true,"width":4000000,"height":6000000,"pins":[{"number":"1","name":"VIN","position":{"x":-800001,"y":123},"electrical":"power_input"}]}})
}
fn create(engine: &EngineHandle, who: Origin) -> ObjectId {
    engine.execute("edit_schematic_sheet", json!({"expected_project":engine.read(|p|p.id),"name":"Power","edits":[symbol(),{"kind":"wire","start":{"x":0,"y":0},"end":{"x":1000000,"y":0}},{"kind":"junction","position":{"x":0,"y":0}},{"kind":"label","position":{"x":0,"y":0},"name":"VIN","scope":"power"}]}),who).unwrap().created[0]
}
#[test]
fn sheet_atomic_guards_pin_identity_paging_and_history_roundtrip() {
    let engine = EngineHandle::new(Project::new("Schematic"));
    let initial = engine.session_state();
    let a = Origin::agent("a");
    let id = create(&engine, a.clone());
    let created = engine.session_state();
    let sheet = created.project.schematic.sheets[&id].clone();
    let content = sheet.content.as_ref().unwrap();
    let s = content.symbols.values().next().unwrap();
    let pin = s.pins[0].id;
    assert_eq!(s.pin_position(s.pins[0].position).unwrap().y.0, 19200001);
    let list = engine.query_schematic(&SchematicQuery::default()).unwrap();
    assert_eq!((list.total, list.sheets[0].object_count, list.sheets[0].pin_count), (1, 4, 1));
    let mut ids = vec![];
    for offset in 0..4 {
        let page = engine.query_schematic(&SchematicQuery { sheet_id: Some(id), offset, limit: Some(1) }).unwrap();
        assert_eq!(page.total, 4);
        assert_eq!(page.next_offset, (offset < 3).then_some(offset + 1));
        ids.push(page.objects[0].id());
    }
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(engine.session_state(), created, "queries preserve all session state");
    let mut edit = symbol();
    edit["symbol"]["id"] = json!(s.id);
    edit["symbol"]["value"] = json!("Updated");
    edit["symbol"]["pins"].as_array_mut().unwrap().push(json!({"number":"2","name":"GND","position":{"x":1000000,"y":0},"electrical":"power_input"}));
    let input = json!({"expected_project":created.project.id,"sheet_id":id,"expected":sheet,"name":"Power","edits":[edit]});
    engine.execute("edit_schematic_sheet", input.clone(), a.clone()).unwrap();
    let updated = engine.session_state();
    let new_sheet = &updated.project.schematic.sheets[&id];
    let new_symbol = &new_sheet.content.as_ref().unwrap().symbols[&s.id];
    assert_eq!(new_symbol.pins[0].id, pin);
    assert_ne!(new_symbol.pins[1].id, pin);
    assert!(engine.execute("edit_schematic_sheet", input, a.clone()).is_err(), "stale guard");
    let status = engine.document_status();
    let len = engine.with_engine(|e| e.log().len());
    for (project, expected, edits) in [
        (
            updated.project.id,
            new_sheet.clone(),
            json!([{"kind":"junction","position":{"x":3,"y":4}},{"kind":"wire","start":{"x":0,"y":0},"end":{"x":0,"y":0}}]),
        ),
        (Project::new("other").id, new_sheet.clone(), json!([])),
        (updated.project.id, sheet.clone(), json!([])),
        (updated.project.id, new_sheet.clone(), json!([{"kind":"delete","id":Project::new("missing").id}])),
    ] {
        assert!(
            engine
                .execute(
                    "edit_schematic_sheet",
                    json!({"expected_project":project,"sheet_id":id,"expected":expected,"name":"Power","edits":edits}),
                    a.clone()
                )
                .is_err()
        );
        assert_eq!(engine.session_state(), updated);
        assert_eq!(engine.document_status(), status);
        assert_eq!(engine.with_engine(|e| e.log().len()), len);
    }
    engine
        .execute(
            "edit_schematic_sheet",
            json!({"expected_project":updated.project.id,"sheet_id":id,"expected":new_sheet,"name":"Power","edits":[]}),
            a.clone(),
        )
        .unwrap();
    assert_eq!(engine.session_state(), updated, "no-op produces no transaction");
    engine.undo(a.clone()).unwrap();
    assert_eq!(engine.session_state().project, created.project);
    engine.redo(a.clone()).unwrap();
    assert_eq!(engine.session_state().project, updated.project);
    engine.execute("delete_schematic_sheet", json!({"expected_project":updated.project.id,"expected":new_sheet}), a.clone()).unwrap();
    assert_eq!(engine.session_state().project, initial.project);
    engine.undo(a).unwrap();
    let mut bytes = vec![];
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), updated.project);
    assert_eq!(file::from_str(&file::to_string(&updated.project)).unwrap(), updated.project);
    let text = String::from_utf8(bytes).unwrap();
    assert!(EventLog::read_jsonl(text.replace("\"version\":6", "\"version\":4").as_bytes()).is_err());
    let mut lines: Vec<Value> = text.lines().map(|s| serde_json::from_str(s).unwrap()).collect();
    lines[1]["deltas"][0]["after"]["content"]["symbols"][s.id.to_string()]["rotation_quarters"] = json!(4);
    let corrupt = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    assert!(EventLog::read_jsonl(corrupt.as_bytes()).is_err());
}
#[test]
fn selective_undo_distinguishes_sheets_and_detects_same_sheet_conflict() {
    let engine = EngineHandle::new(Project::new("Shared"));
    let a = Origin::agent("a");
    let b = Origin::agent("b");
    let id = create(&engine, a.clone());
    let second = engine
        .execute("edit_schematic_sheet", json!({"expected_project":engine.read(|p|p.id),"name":"Other","edits":[]}), b.clone())
        .unwrap()
        .created[0];
    engine.undo_own(a.clone()).unwrap();
    assert!(engine.read(|p| !p.schematic.sheets.contains_key(&id) && p.schematic.sheets.contains_key(&second)));
    engine.redo_own(a.clone()).unwrap();
    let sheet = engine.read(|p| p.schematic.sheets[&id].clone());
    engine
        .execute(
            "edit_schematic_sheet",
            json!({"expected_project":engine.read(|p|p.id),"sheet_id":id,"expected":sheet,"name":"Renamed","edits":[]}),
            b,
        )
        .unwrap();
    assert!(engine.undo_own(a).is_err());
}

#[test]
fn log_base_rejects_sheet_identity_key_mismatch() {
    let engine = EngineHandle::new(Project::new("Bad base"));
    let id = create(&engine, Origin::User);
    let mut project = engine.session_state().project;
    project.schematic.sheets.get_mut(&id).unwrap().id = ObjectId::new();
    let mut bytes = vec![];
    EventLog::new(project).write_jsonl(&mut bytes).unwrap();
    assert!(EventLog::read_jsonl(bytes.as_slice()).is_err());
}
