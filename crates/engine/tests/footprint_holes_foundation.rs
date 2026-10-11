use viamind_doc::{ObjectId, PartHole, file};
use viamind_engine::{ChangeEvent, ChangeKind, Delta, EventLog, Origin, TxId};
use viamind_geom::{Nm, Point};

#[test]
fn log15_base_before_after_and_undo_redo_preserve_exact_drill_intent() {
    let base = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let id = *base.board.parts.keys().next().unwrap();
    let before = base.board.parts[&id].clone();
    let mut after = before.clone();
    after.holes = Some(vec![PartHole { id: ObjectId::new(), key: "mount".into(), center: Point::new(Nm(3), Nm(-5)), diameter: Nm(7) }]);
    let delta = Delta::Part { id, before: Some(before.clone()), after: Some(after.clone()) };
    let event = |kind, deltas| ChangeEvent { tx: TxId(1), origin: Origin::agent("foundation"), kind, deltas };
    let mut log = EventLog::new(base.clone());
    log.append(event(ChangeKind::Applied, vec![delta.clone()]));
    log.append(event(ChangeKind::Undone, vec![Delta::Part { id, before: Some(after.clone()), after: Some(before.clone()) }]));
    log.append(event(ChangeKind::Redone, vec![delta]));
    let mut bytes = vec![];
    log.write_jsonl(&mut bytes).unwrap();
    let replay = EventLog::read_jsonl(bytes.as_slice()).unwrap();
    assert_eq!(replay, log);
    assert_eq!(replay.replay().board.parts[&id], after);
    let text = String::from_utf8(bytes).unwrap();
    assert!(EventLog::read_jsonl(text.replacen("\"version\":15", "\"version\":14", 1).as_bytes()).is_err());
    let mut base15 = base.clone();
    base15.board.parts.insert(id, after.clone());
    let mut base_log = Vec::new();
    EventLog::new(base15.clone()).write_jsonl(&mut base_log).unwrap();
    let text = String::from_utf8(base_log).unwrap();
    assert!(EventLog::read_jsonl(text.replacen("\"version\":15", "\"version\":14", 1).as_bytes()).is_err());
    let mut removal = EventLog::new(base15);
    removal.append(event(ChangeKind::Applied, vec![Delta::Part { id, before: Some(after), after: Some(before) }]));
    let mut bytes = vec![];
    removal.write_jsonl(&mut bytes).unwrap();
    let mut lines: Vec<serde_json::Value> = String::from_utf8(bytes).unwrap().lines().map(|s| serde_json::from_str(s).unwrap()).collect();
    lines[0]["version"] = 14.into();
    lines[0]["base"] = serde_json::to_value(base).unwrap();
    let forged = lines.iter().map(|v| format!("{v}\n")).collect::<String>();
    assert!(EventLog::read_jsonl(forged.as_bytes()).is_err());
}

#[test]
fn log_rejects_an_intermediate_global_hole_collision_even_if_later_removed() {
    let base = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let id = *base.board.parts.keys().next().unwrap();
    let before = base.board.parts[&id].clone();
    let mut after = before.clone();
    after.holes = Some(vec![PartHole { id: base.id, key: "collision".into(), center: Point::ORIGIN, diameter: Nm(1) }]);
    let mut log = EventLog::new(base);
    for (a, b) in [(before.clone(), after.clone()), (after, before)] {
        log.append(ChangeEvent {
            tx: TxId(1),
            origin: Origin::User,
            kind: ChangeKind::Applied,
            deltas: vec![Delta::Part { id, before: Some(a), after: Some(b) }],
        });
    }
    let mut bytes = vec![];
    log.write_jsonl(&mut bytes).unwrap();
    assert!(EventLog::read_jsonl(bytes.as_slice()).is_err());
}
