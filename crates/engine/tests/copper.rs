use serde_json::{Value, json};
use viamind_doc::{Project, file};
use viamind_engine::{CommandCall, EngineHandle, EventLog, Origin};
fn track() -> Value {
    json!({"start_x_mm":-0.000001,"start_y_mm":2,"end_x_mm":1.234567,"end_y_mm":-3.456789,"width_mm":0.250001,"layer":"bottom","net":"GND"})
}
fn via() -> Value {
    json!({"x_mm":1.234567,"y_mm":-3.456789,"diameter_mm":0.6,"drill_mm":0.3,"net":"GND"})
}
#[test]
fn copper_create_delete_undo_redo_replay_and_guarded_failures() {
    let engine = EngineHandle::new(Project::new("Copper"));
    let initial = engine.session_state();
    let agent = Origin::agent("router");
    let track_id = engine.execute("add_track", track(), agent.clone()).unwrap().created[0];
    let via_id = engine.execute("add_via", via(), agent.clone()).unwrap().created[0];
    let full = engine.session_state().project;
    assert_eq!(full.board.tracks[&track_id].width.0, 250_001);
    assert_eq!(full.board.tracks[&track_id].start.x.0, -1);
    let status = engine.document_status();
    let checkpoint = engine.session_state();
    let len = engine.with_engine(|e| e.log().len());
    for change in [
        json!({"width_mm":0}),
        json!({"width_mm":0.0000001}),
        json!({"layer":"inner1"}),
        json!({"net":"\n"}),
        json!({"start_x_mm":10001}),
        json!({"expected_project":Project::new("other").id}),
        json!({"end_x_mm":-0.000001,"end_y_mm":2}),
    ] {
        let mut input = track();
        input.as_object_mut().unwrap().extend(change.as_object().unwrap().clone());
        assert!(engine.execute("add_track", input, agent.clone()).is_err());
    }
    for change in [
        json!({"drill_mm":0}),
        json!({"drill_mm":0.6}),
        json!({"diameter_mm":-1}),
        json!({"net":" bad "}),
        json!({"type":"blind"}),
        json!({"diameter_mm":0.0000001}),
    ] {
        let mut input = via();
        input.as_object_mut().unwrap().extend(change.as_object().unwrap().clone());
        assert!(engine.execute("add_via", input, agent.clone()).is_err());
    }
    let mut stale = full.board.tracks[&track_id].clone();
    stale.width.0 += 1;
    assert!(engine.execute("delete_track", json!({"id":track_id,"expected":stale}), agent.clone()).is_err());
    assert_eq!(engine.session_state(), checkpoint);
    assert_eq!(engine.document_status(), status);
    assert_eq!(engine.with_engine(|e| e.log().len()), len);
    engine.execute("delete_track", json!({"id":track_id,"expected":full.board.tracks[&track_id]}), agent.clone()).unwrap();
    engine.execute("delete_via", json!({"id":via_id,"expected":full.board.vias[&via_id]}), agent.clone()).unwrap();
    assert_eq!(engine.session_state().project, initial.project);
    engine.undo(Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, full);
    engine.redo(Origin::User).unwrap();
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, initial.project);
    engine.undo(Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    let replay = EventLog::read_jsonl(bytes.as_slice()).unwrap();
    assert_eq!(replay.replay(), full);
    assert_eq!(file::from_str(&file::to_string(&full)).unwrap(), full);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"version\":7"));
    assert!(EventLog::read_jsonl(text.replace("\"version\":7", "\"version\":3").as_bytes()).is_err());
}
#[test]
fn failed_batch_and_agent_undo_preserve_other_copper() {
    let engine = EngineHandle::new(Project::new("Batch"));
    let base = engine.session_state();
    assert!(
        engine
            .execute_batch(vec![CommandCall::new("add_track", track()), CommandCall::new("add_via", json!({"drill_mm":0}))], Origin::User, None)
            .is_err()
    );
    assert_eq!(engine.session_state(), base);
    let a = Origin::agent("a");
    let b = Origin::agent("b");
    let id = engine.execute("add_track", track(), a.clone()).unwrap().created[0];
    let v = engine.execute("add_via", via(), b.clone()).unwrap().created[0];
    engine.undo_own(a.clone()).unwrap();
    assert!(engine.read(|p| !p.board.tracks.contains_key(&id) && p.board.vias.contains_key(&v)));
    engine.redo_own(a.clone()).unwrap();
    engine.execute("delete_track", json!({"id":id}), b).unwrap();
    assert!(engine.undo_own(a).is_err(), "same-object cross-agent conflict");
}

#[test]
fn malformed_log_copper_identity_or_native_collision_is_rejected() {
    let engine = EngineHandle::new(Project::new("Log identity"));
    engine.execute("add_track", track(), Origin::User).unwrap();
    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<_> = text.lines().collect();
    let mut entry: Value = serde_json::from_str(lines[1]).unwrap();
    let foreign = Project::new("Foreign").id;
    entry["deltas"][0]["id"] = json!(foreign);
    assert!(EventLog::read_jsonl(format!("{}\n{}\n", lines[0], entry).as_bytes()).is_err());
    let base = engine.with_engine(|e| e.log().base().id);
    entry["deltas"][0]["id"] = json!(base);
    entry["deltas"][0]["after"]["id"] = json!(base);
    assert!(EventLog::read_jsonl(format!("{}\n{}\n", lines[0], entry).as_bytes()).is_err());
}
