//! Electrical geometry survives history, persistence and concurrent authors.
use serde_json::{Value, json};
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, EventLog, Origin};
fn input(number: &str) -> Value {
    json!({"number":number,"x_mm":-0.800001,"y_mm":0,"width_mm":0.9,"height_mm":0.95,"net":" GND "})
}
#[test]
fn pad_identities_history_versioning_and_atomic_guards() {
    let engine = EngineHandle::new(Project::new("Pads"));
    let id =
        engine.execute("add_part", json!({"designator":"R1","value":"10k","footprint":"R_0603","x_mm":10,"y_mm":20}), Origin::User).unwrap().created
            [0];
    let before = engine.read(|p| p.board.part(&id).unwrap().clone());
    let command = |expected: &viamind_doc::Part, pads: Value| json!({"id":id,"expected":expected,"pads":pads});
    engine.execute("set_part_pads", command(&before, json!([input("1"), input("2")])), Origin::agent("pads")).unwrap();
    let after = engine.read(Clone::clone);
    let part = after.board.part(&id).unwrap();
    let pads = &part.pads.as_ref().unwrap().0;
    assert_eq!(pads[0].position.x.0, -800001);
    assert_eq!(pads[0].net.as_deref(), Some("GND"));
    assert_ne!(pads[0].id, pads[1].id);
    let text = file::to_string(&after);
    assert!(text.contains("\"version\": 3"));
    assert_eq!(file::from_str(&text).unwrap(), after);
    assert!(file::from_str(&text.replace("\"version\": 3", "\"version\": 2")).is_err());
    let snapshot = engine.session_state();
    let mut invalid = input("1");
    invalid["width_mm"] = json!(0.0000001);
    for call in [
        command(&before, json!([])),
        command(part, json!([input("1"), input("1")])),
        command(part, json!([invalid])),
        command(part, json!([{ "id":before.id, "number":"x", "x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1 }])),
    ] {
        assert!(engine.execute("set_part_pads", call, Origin::User).is_err());
        assert_eq!(engine.session_state(), snapshot);
    }
    let noop = engine.execute("set_part_pads", command(part, json!([input("1"), input("2")])), Origin::User).unwrap();
    assert!(noop.deltas.is_empty());
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().clone()), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    engine.execute("delete_part", json!({"id":id}), Origin::User).unwrap();
    assert!(engine.read(|p| p.board.parts.is_empty()));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), after);
    let old_log = String::from_utf8(bytes).unwrap().replace(&format!("\"version\":{}", viamind_engine::log::LOG_VERSION), "\"version\":2");
    assert!(EventLog::read_jsonl(old_log.as_bytes()).is_err(), "old log header must not hide appended pads");
    let mut bad: Value = serde_json::from_str(&text).unwrap();
    bad["project"]["board"]["parts"][id.to_string()]["pads"][0]["id"] = json!(id);
    assert!(file::from_str(&bad.to_string()).is_err());
    bad["project"]["board"]["parts"][id.to_string()]["pads"][0]["id"] = json!(pads[0].id);
    bad["project"]["board"]["parts"][id.to_string()]["pads"][0]["width"] = json!(-1);
    assert!(file::from_str(&bad.to_string()).is_err());
}
#[test]
fn editing_a_pad_blocks_selective_undo_of_its_part_creation() {
    let engine = EngineHandle::new(Project::new("Concurrent pads"));
    let origin = Origin::agent("placer");
    let id =
        engine.execute("add_part", json!({"designator":"R1","value":"10k","footprint":"R_0603","x_mm":0,"y_mm":0}), origin.clone()).unwrap().created
            [0];
    let part = engine.read(|p| p.board.part(&id).unwrap().clone());
    engine.execute("set_part_pads", json!({"id":id,"expected":part,"pads":[input("1")]}), Origin::agent("electrical")).unwrap();
    let state = engine.session_state();
    assert!(engine.undo_own(origin).is_err());
    assert_eq!(engine.session_state(), state);
}
