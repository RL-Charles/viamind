//! The event log replays to a byte-identical project.

use serde_json::json;
use viamind_doc::{Project, file};
use viamind_engine::log::{write_entry, write_header};
use viamind_engine::{AddPart, CommandCall, DeletePart, EngineHandle, EventLog, LogError, MovePart, Origin};

/// Small deterministic generator so the test needs no extra dependency.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

fn add(designator: String, x_mm: f64) -> AddPart {
    AddPart {
        expected_project: None,
        designator,
        value: "4.7k".into(),
        footprint: "R_0805".into(),
        x_mm,
        y_mm: -x_mm,
        rotation_deg: Some(x_mm * 3.0),
        side: None,
    }
}

/// Drives a mix of commands, batches, failures, undo and redo from two origins.
/// Returns the project after each step together with the log length at that point.
fn drive(engine: &EngineHandle, steps: usize) -> Vec<(usize, Project)> {
    let mut rng = Lcg(7);
    let mut checkpoints = Vec::new();
    for step in 0..steps {
        let origin = if rng.below(2) == 0 { Origin::User } else { Origin::agent("replay-agent") };
        let ids: Vec<_> = engine.read(|p| p.board.parts.keys().copied().collect());
        let pick = |rng: &mut Lcg| ids[rng.below(ids.len() as u64) as usize];
        match rng.below(8) {
            _ if ids.is_empty() => drop(engine.execute_typed(add(format!("R{step}"), step as f64), origin)),
            0..=2 => drop(engine.execute_typed(add(format!("R{step}"), step as f64), origin)),
            3 => drop(engine.execute_typed(MovePart { id: pick(&mut rng), x_mm: step as f64 / 3.0, y_mm: 1.0 }, origin)),
            4 => drop(engine.execute_typed(DeletePart { id: pick(&mut rng) }, origin)),
            5 => {
                let calls = vec![
                    CommandCall::new(
                        "add_part",
                        json!({ "designator": format!("C{step}"), "value": "1u", "footprint": "C_0603", "x_mm": 1, "y_mm": 2 }),
                    ),
                    CommandCall::new("move_part", json!({ "id": pick(&mut rng), "x_mm": -4.25, "y_mm": step })),
                ];
                drop(engine.execute_batch(calls, origin, None));
            }
            6 => drop(engine.undo(origin)),
            _ => drop(engine.redo(origin)),
        }
        // Some of these fail (e.g. duplicate designators); failures must not reach the log.
        let _ = engine.execute_typed(add("R0".into(), 0.0), Origin::User);
        let len = engine.with_engine(|e| e.log().len());
        checkpoints.push((len, engine.read(Project::clone)));
    }
    checkpoints
}

#[test]
fn replaying_the_log_rebuilds_a_byte_identical_project() {
    let engine = EngineHandle::new(Project::new("replay"));
    let checkpoints = drive(&engine, 2_000);

    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    let log = EventLog::read_jsonl(bytes.as_slice()).unwrap();
    assert!(log.len() > 1_000, "the run produced a long log ({})", log.len());

    let current = engine.read(file::to_string);
    assert_eq!(file::to_string(&log.replay()), current, "replay is byte-identical to the live project");

    // Every intermediate state is reproducible too.
    for (len, project) in checkpoints.iter().step_by(97) {
        assert_eq!(file::to_string(&log.replay_until(*len)), file::to_string(project));
    }

    // Writing the log read back from disk gives the same bytes.
    let mut again = Vec::new();
    log.write_jsonl(&mut again).unwrap();
    assert_eq!(again, bytes);
}

/// A subscriber appending each event as it arrives produces the same file as
/// writing the engine's log at the end.
#[test]
fn streaming_append_matches_the_engine_log() {
    let engine = EngineHandle::new(Project::new("stream"));
    let mut streamed = Vec::new();
    write_header(&mut streamed, &engine.read(Project::clone)).unwrap();
    let events = engine.subscribe();

    drive(&engine, 300);
    for event in events.try_iter() {
        write_entry(&mut streamed, &event).unwrap();
    }

    let mut whole = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut whole)).unwrap();
    assert_eq!(streamed, whole);
}

#[test]
fn rejects_foreign_logs() {
    assert!(matches!(EventLog::read_jsonl(&b""[..]), Err(LogError::BadHeader)));
    let err = EventLog::read_jsonl(&br#"{"format":"other","version":1,"base":null}"#[..]).unwrap_err();
    assert!(matches!(err, LogError::Parse { line: 1, .. }), "{err}");
    let mut bytes = Vec::new();
    EventLog::new(Project::new("p")).write_jsonl(&mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap().replace("\"version\":6", "\"version\":99");
    assert!(matches!(EventLog::read_jsonl(text.as_bytes()), Err(LogError::BadHeader)));
    let err = EventLog::read_jsonl(format!("{}\n\n{{\"tx\":1}}\n", text.replace("\"version\":99", "\"version\":1").trim()).as_bytes()).unwrap_err();
    assert!(matches!(err, LogError::Parse { line: 3, .. }), "{err}");
}
