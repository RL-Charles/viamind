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
fn calibration() -> Value {
    json!({"mounting":"surface_mount","placement":{"centroid_local":{"x":2_000_000,"y":1_000_000},"zero_axis_mdeg":37_125}})
}
#[test]
fn calibration_is_guarded_versioned_undoable_and_every_historical_record_requires_nine() {
    let (engine, id) = sample();
    let original = engine.read(|p| p.board.parts[&id].clone());
    let before = engine.read(Clone::clone);
    engine
        .execute(
            "set_part_assembly",
            json!({"id":id,"assembly":calibration(),"expected":original,"expected_project":before.id}),
            Origin::agent("first"),
        )
        .unwrap();
    let after = engine.read(Clone::clone);
    assert_eq!(file::minimum_version(&after), 9);
    let serialized = file::to_string(&after);
    assert_eq!(file::from_str(&serialized).unwrap(), after);
    let mut downgraded: Value = serde_json::from_str(&serialized).unwrap();
    downgraded["version"] = json!(8);
    assert!(file::from_str(&downgraded.to_string()).is_err());
    assert_eq!(engine.query_parts(&PartQuery::default()).unwrap().parts[0].assembly, after.board.parts[&id].assembly);
    let checkpoint = engine.session_state();
    assert!(engine.execute("set_part_assembly", json!({"id":id,"assembly":null,"expected":original}), Origin::agent("stale")).is_err());
    assert_eq!(engine.session_state(), checkpoint);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    engine.execute("update_part", json!({"id":id,"side":"bottom","rotation_deg":33.7}), Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.parts[&id].assembly.clone()), after.board.parts[&id].assembly);
    engine.execute("set_part_assembly", json!({"id":id,"assembly":null}), Origin::User).unwrap();
    assert_eq!(engine.read(file::minimum_version), 1);
    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), engine.read(Clone::clone));
    let text = String::from_utf8(bytes).unwrap();
    let old = text.replace(&format!("\"version\":{}", viamind_engine::log::LOG_VERSION), "\"version\":8");
    assert!(EventLog::read_jsonl(old.as_bytes()).is_err(), "cleared intermediate calibration must retain its schema requirement");
    // Clearing the only calibration in a version8 base cannot hide a before snapshot either.
    let engine = EngineHandle::new(after.clone());
    engine.execute("set_part_assembly", json!({"id":id,"assembly":null}), Origin::User).unwrap();
    let event = engine.with_engine(|e| e.log().entries().last().unwrap().clone());
    let mut base = before;
    let mut header = json!({"format":"viamind-log","version":8,"base":base});
    let old = format!("{header}\n{}\n", serde_json::to_string(&event).unwrap());
    assert!(EventLog::read_jsonl(old.as_bytes()).is_err());
    base = after;
    header["base"] = serde_json::to_value(base).unwrap();
    assert!(EventLog::read_jsonl(format!("{header}\n").as_bytes()).is_err());
}

#[test]
fn invalid_calibration_and_missing_mounting_never_mutate_or_clone_into_queries() {
    let (engine, id) = sample();
    let before = engine.session_state();
    for metadata in [
        json!({"mounting":"unknown"}),
        json!({"placement":{}}),
        json!({"placement":{"centroid_local":{"x":10_000_000_001_i64,"y":0},"zero_axis_mdeg":0}}),
        json!({"placement":{"centroid_local":{"x":0,"y":0},"zero_axis_mdeg":360_000}}),
    ] {
        assert!(engine.execute("set_part_assembly", json!({"id":id,"assembly":metadata}), Origin::User).is_err());
        assert_eq!(engine.session_state(), before);
    }
    // Both independent optional fields are legitimate persisted data; the export enforces completeness.
    for metadata in [json!({"mounting":"hybrid"}), json!({"placement":{"centroid_local":{"x":0,"y":0},"zero_axis_mdeg":0}})] {
        engine.execute("set_part_assembly", json!({"id":id,"assembly":metadata}), Origin::User).unwrap();
        assert_eq!(engine.read(file::minimum_version), 9);
    }
    let mut project = engine.read(Clone::clone);
    project.board.parts.get_mut(&id).unwrap().assembly.as_mut().unwrap().placement.as_mut().unwrap().zero_axis_mdeg = -1;
    assert!(PartQuery::default().run(&project).is_err());
    let mut output = b"unchanged".to_vec();
    assert!(viamind_engine::log::write_header(&mut output, &project).is_err());
    assert_eq!(output, b"unchanged");
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
    lines[0]["version"] = json!(viamind_engine::log::LOG_VERSION);
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
    project.board.parts.get_mut(&id).unwrap().assembly = Some(Box::new(AssemblyMetadata { mpn: Some("x".repeat(513)), ..Default::default() }));
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
    project.board.parts.get_mut(&id).unwrap().assembly = Some(Box::new(valid));
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

#[test]
fn grouped_repeated_part_events_replay_and_oversized_before_records_never_write() {
    let (engine, id) = sample();
    let metadata = AssemblyMetadata {
        manufacturer: Some("a".repeat(512)),
        mpn: Some("b".repeat(512)),
        supplier: Some("c".repeat(512)),
        supplier_sku: Some("d".repeat(512)),
        populate: true,
        ..Default::default()
    };
    engine.execute("set_part_assembly", json!({"id":id,"assembly":metadata}), Origin::User).unwrap();
    let project = engine.read(Clone::clone);
    let part = project.board.part(&id).unwrap().clone();
    let mut event = engine.with_engine(|e| e.log().entries().last().unwrap().clone());
    // More than 2 MiB of repeated after snapshots represent only one 2 KiB state.
    event.deltas = (0..1025).map(|_| viamind_engine::Delta::Part { id, before: Some(part.clone()), after: Some(part.clone()) }).collect();
    let mut bytes = Vec::new();
    viamind_engine::log::write_header(&mut bytes, &project).unwrap();
    viamind_engine::log::write_entry(&mut bytes, &event).unwrap();
    assert_eq!(EventLog::read_jsonl(std::io::Cursor::new(bytes)).unwrap().replay(), project);
    event.deltas = (0..1025)
        .map(|_| {
            let mut p = part.clone();
            p.id = ObjectId::new();
            viamind_engine::Delta::Part { id: p.id, before: Some(p), after: None }
        })
        .collect();
    let mut output = b"preserve existing output".to_vec();
    assert!(viamind_engine::log::write_entry(&mut output, &event).is_err());
    assert_eq!(output, b"preserve existing output");
    let mut bad = Vec::new();
    viamind_engine::log::write_header(&mut bad, &Project::new("base")).unwrap();
    serde_json::to_writer(&mut bad, &event).unwrap();
    bad.push(b'\n');
    assert!(EventLog::read_jsonl(std::io::Cursor::new(bad)).is_err());
    event.deltas.truncate(1);
    if let viamind_engine::Delta::Part { before: Some(p), .. } = &mut event.deltas[0] {
        p.assembly.as_mut().unwrap().mpn = Some("x".repeat(513));
    }
    assert!(viamind_engine::log::write_entry(&mut output, &event).is_err());
    assert_eq!(output, b"preserve existing output");
}
