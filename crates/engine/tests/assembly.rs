use serde_json::{Value, json};
use viamind_doc::{AssemblyMetadata, ObjectId, Project, file};
use viamind_engine::{EngineHandle, EventLog, Origin, PartQuery};

fn sample() -> (EngineHandle, ObjectId) {
    let engine = EngineHandle::new(Project::new("Sourcing"));
    let id =
        engine.execute("add_part", json!({"designator":"R1","value":"10k","footprint":"R_0603","x_mm":1,"y_mm":2}), Origin::User).unwrap().created[0];
    (engine, id)
}
fn metadata() -> Value {
    json!({"manufacturer":"µ Parts, \"demo\"","mpn":"USER-ENTERED","supplier":"Example supplier","supplier_sku":"USER-SKU","populate":false})
}
#[test]
fn guarded_sourcing_roundtrips_native_log_and_history_and_preserves_other_edits() {
    let (engine, id) = sample();
    let before = engine.read(Clone::clone);
    let original = before.board.part(&id).unwrap();
    engine
        .execute("set_part_assembly", json!({"id":id,"assembly":metadata(),"expected":original,"expected_project":before.id}), Origin::User)
        .unwrap();
    let after = engine.read(Clone::clone);
    assert_eq!(file::minimum_version(&after), 7);
    assert_eq!(file::from_str(&file::to_string(&after)).unwrap(), after);
    let mut downgraded: Value = serde_json::from_str(&file::to_string(&after)).unwrap();
    downgraded["version"] = json!(6);
    assert!(file::from_str(&downgraded.to_string()).is_err());
    assert_eq!(engine.query_parts(&PartQuery::default()).unwrap().parts[0].assembly, after.board.part(&id).unwrap().assembly);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    engine.execute("update_part", json!({"id":id,"x_mm":4}), Origin::agent("peer")).unwrap();
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().assembly.clone()), after.board.part(&id).unwrap().assembly);
    engine.execute("set_part_assembly", json!({"id":id,"assembly":null}), Origin::User).unwrap();
    assert_eq!(engine.read(file::minimum_version), 1);
    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    assert_eq!(EventLog::read_jsonl(std::io::Cursor::new(&bytes)).unwrap().replay(), engine.read(Clone::clone));
    // Intermediate metadata must not disappear unnoticed when the final record is clear.
    let mut lines: Vec<Value> = String::from_utf8(bytes).unwrap().lines().map(|s| serde_json::from_str(s).unwrap()).collect();
    lines[0]["version"] = json!(6);
    let old = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    assert!(EventLog::read_jsonl(std::io::Cursor::new(old)).is_err());
    lines[0]["version"] = json!(7);
    let event = lines.iter_mut().skip(1).find(|v| v["deltas"][0]["after"]["assembly"].is_object()).unwrap();
    event["deltas"][0]["after"]["assembly"]["mpn"] = json!("x".repeat(513));
    let bad = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    assert!(EventLog::read_jsonl(std::io::Cursor::new(bad)).is_err());
}
#[test]
fn invalid_stale_unknown_and_default_records_are_atomic() {
    let (engine, id) = sample();
    let original = engine.read(|p| p.board.part(&id).unwrap().clone());
    engine.execute("update_part", json!({"id":id,"value":"peer"}), Origin::agent("peer")).unwrap();
    let before = engine.session_state();
    for assembly in [json!({"mpn":""}), json!({"mpn":" x"}), json!({"mpn":"x\n"}), json!({"mpn":"é".repeat(257)}), json!({"unknown":"x"})] {
        assert!(engine.execute("set_part_assembly", json!({"id":id,"assembly":assembly}), Origin::User).is_err());
        assert_eq!(engine.session_state(), before);
    }
    for guard in [json!({"expected":original}), json!({"expected_project":ObjectId::new()})] {
        let mut input = json!({"id":id,"assembly":metadata()});
        input.as_object_mut().unwrap().extend(guard.as_object().unwrap().clone());
        assert!(engine.execute("set_part_assembly", input, Origin::User).is_err());
    }
    assert!(engine.execute("set_part_assembly", json!({"id":id}), Origin::User).is_err());
    let no_op = engine.execute("set_part_assembly", json!({"id":id,"assembly":{}}), Origin::User).unwrap();
    assert!(no_op.deltas.is_empty());
    assert_eq!(engine.session_state(), before);
}
#[test]
fn model_budget_and_invalid_returned_query_fail_before_copy_or_native_publication() {
    let (engine, id) = sample();
    let mut project = engine.read(Clone::clone);
    project.board.parts.get_mut(&id).unwrap().assembly = Some(AssemblyMetadata { mpn: Some("x".repeat(513)), ..Default::default() });
    assert!(PartQuery::default().run(&project).is_err());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native.json");
    std::fs::write(&path, "keep").unwrap();
    assert!(file::save(&project, &path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
    let valid = AssemblyMetadata {
        manufacturer: Some("x".repeat(512)),
        mpn: Some("m".repeat(512)),
        supplier: Some("s".repeat(512)),
        supplier_sku: Some("k".repeat(512)),
        ..Default::default()
    };
    project.board.parts.get_mut(&id).unwrap().assembly = Some(valid);
    let first = project.board.part(&id).unwrap().clone();
    for _ in 0..1024 {
        let mut part = first.clone();
        part.id = ObjectId::new();
        project.board.parts.insert(part.id, part);
    }
    assert!(file::validate_assembly(&project).is_err());
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    assert!(engine.execute("set_part_assembly", json!({"id":id,"assembly":metadata()}), Origin::User).is_err());
    assert_eq!(engine.session_state(), before);
    assert!(engine.save_project(viamind_engine::SaveProject { path: Some(path), overwrite: true }).is_err());
}
