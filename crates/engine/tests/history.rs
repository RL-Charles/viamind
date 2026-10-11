//! Undo/redo history through `EngineHandle`: long histories, origins and threads.

use std::time::Instant;

use serde_json::json;
use viamind_doc::{ObjectId, Project};
use viamind_engine::{AddPart, DeletePart, EngineHandle, MovePart, Origin, TxId};

fn add(designator: String, x_mm: f64) -> AddPart {
    AddPart {
        expected_project: None,
        designator,
        value: "1k".into(),
        footprint: "R_0603".into(),
        x_mm,
        y_mm: 0.0,
        rotation_deg: Some(x_mm),
        side: None,
    }
}

/// Small deterministic generator so the test needs no extra dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

#[test]
fn ten_thousand_step_history() {
    const STEPS: usize = 10_000;
    let engine = EngineHandle::new(Project::new("history"));
    let initial = engine.read(Project::clone);
    let mut rng = Lcg(42);
    let mut live: Vec<ObjectId> = Vec::new();
    // Snapshot of the project after every 1,000th step, to check intermediate states.
    let mut checkpoints = Vec::new();

    let started = Instant::now();
    for step in 0..STEPS {
        let origin = if step % 3 == 0 { Origin::agent("agent-1") } else { Origin::User };
        let roll = rng.next() % 10;
        let result = if live.is_empty() || roll < 4 {
            let result = engine.execute_typed(add(format!("R{step}"), (step % 500) as f64), origin).unwrap();
            live.push(result.created[0]);
            result
        } else if roll < 8 {
            let id = live[rng.next() as usize % live.len()];
            engine.execute_typed(MovePart { id, x_mm: (rng.next() % 1000) as f64 / 7.0, y_mm: step as f64 / 10.0 }, origin).unwrap()
        } else {
            let id = live.swap_remove(rng.next() as usize % live.len());
            engine.execute_typed(DeletePart { id }, origin).unwrap()
        };
        assert_eq!(result.deltas.len(), 1, "every step changes exactly one part");
        if (step + 1) % 1_000 == 0 {
            checkpoints.push(engine.read(Project::clone));
        }
    }
    let applied = started.elapsed();
    let final_state = engine.read(Project::clone);
    assert_eq!(engine.with_engine(|e| e.undo_history().len()), STEPS);

    let started = Instant::now();
    for undone in 0..STEPS {
        if undone % 1_000 == 0 {
            let checkpoint = &checkpoints[checkpoints.len() - 1 - undone / 1_000];
            assert!(engine.read(|p| p == checkpoint), "state before undo {undone} matches checkpoint");
        }
        assert!(engine.undo(Origin::User).unwrap().is_some());
    }
    let undo_time = started.elapsed();
    assert_eq!(engine.undo(Origin::User).unwrap(), None);
    assert!(engine.read(|p| *p == initial), "undoing every step restores the initial project exactly");

    let started = Instant::now();
    while engine.redo(Origin::User).unwrap().is_some() {}
    let redo_time = started.elapsed();
    assert!(engine.read(|p| *p == final_state), "redoing every step restores the final project exactly");

    eprintln!("10k steps: apply {applied:?}, undo {undo_time:?}, redo {redo_time:?}");
}

#[test]
fn user_undoes_an_agent_edit() {
    let engine = EngineHandle::new(Project::new("p"));
    let agent = Origin::agent("claude");
    let input = json!({ "designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 10, "y_mm": 10 });
    let result = engine.execute("add_part", input, agent.clone()).unwrap();
    let info = engine.next_undo().unwrap();
    assert_eq!((info.id, &info.origin, info.label.as_str()), (result.tx, &agent, "add_part"));

    assert_eq!(engine.undo(Origin::User).unwrap(), Some(result.tx));
    assert!(engine.read(|p| p.board.parts.is_empty()));
    assert_eq!(engine.next_redo().unwrap().origin, agent);
    assert_eq!(engine.redo(Origin::User).unwrap(), Some(result.tx));
    assert!(engine.read(|p| p.board.part(&result.created[0]).is_some()));
}

#[test]
fn handles_are_shared_across_threads() {
    const THREADS: usize = 4;
    const EACH: usize = 250;
    let engine = EngineHandle::new(Project::new("p"));
    let workers: Vec<_> = (0..THREADS)
        .map(|t| {
            let engine = engine.clone();
            std::thread::spawn(move || {
                (0..EACH)
                    .map(|i| engine.execute_typed(add(format!("U{t}_{i}"), i as f64), Origin::agent(format!("a{t}"))).unwrap().tx)
                    .collect::<Vec<TxId>>()
            })
        })
        .collect();
    let mut ids: Vec<TxId> = workers.into_iter().flat_map(|w| w.join().unwrap()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), THREADS * EACH, "transaction ids are unique");
    assert_eq!(engine.read(|p| p.board.parts.len()), THREADS * EACH);
    while engine.undo(Origin::User).unwrap().is_some() {}
    assert!(engine.read(|p| p.board.parts.is_empty()));
}
