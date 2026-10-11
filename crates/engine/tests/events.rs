//! Change notifications: one typed diff per transaction, shared by UI and MCP.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;

use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{AddPart, ChangeEvent, ChangeKind, CommandCall, Engine, EngineHandle, MovePart, Origin, apply_all};

fn add(designator: &str) -> AddPart {
    AddPart {
        expected_project: None,
        designator: designator.into(),
        value: "1k".into(),
        footprint: "R_0603".into(),
        x_mm: 1.0,
        y_mm: 2.0,
        rotation_deg: None,
        side: None,
    }
}

fn drain(receiver: &Receiver<ChangeEvent>) -> Vec<ChangeEvent> {
    receiver.try_iter().collect()
}

#[test]
fn one_typed_diff_per_transaction() {
    let engine = EngineHandle::new(Project::new("p"));
    let events = engine.subscribe();
    let agent = Origin::agent("claude");

    let r1 = engine.execute_typed(add("R1"), agent.clone()).unwrap();
    let [event] = drain(&events).try_into().unwrap();
    assert_eq!(event, ChangeEvent { tx: r1.tx, origin: agent.clone(), kind: ChangeKind::Applied, deltas: r1.deltas.clone() });

    // A batch of three commands is one transaction, so one event (deltas coalesced per object).
    let id = r1.created[0];
    let calls = vec![
        CommandCall::new("move_part", json!({ "id": id, "x_mm": 5, "y_mm": 5 })),
        CommandCall::new("move_part", json!({ "id": id, "x_mm": 6, "y_mm": 6 })),
        CommandCall::new("add_part", json!({ "designator": "R2", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 })),
    ];
    let batch = engine.execute_batch(calls, Origin::User, Some("place")).unwrap();
    let [event] = drain(&events).try_into().unwrap();
    assert_eq!((event.tx, event.kind, event.deltas.len()), (batch.tx, ChangeKind::Applied, 2));

    // Failed and no-op transactions send nothing.
    assert!(engine.execute_typed(add("R1"), Origin::User).is_err());
    engine.execute_typed(MovePart { id, x_mm: 6.0, y_mm: 6.0 }, Origin::User).unwrap();
    assert!(drain(&events).is_empty());

    // Undo and redo report the original transaction, who asked, and the change just made.
    engine.undo(Origin::User).unwrap();
    let [undone] = drain(&events).try_into().unwrap();
    assert_eq!((undone.tx, undone.kind, &undone.origin), (batch.tx, ChangeKind::Undone, &Origin::User));
    assert_eq!(undone.deltas, viamind_engine::invert(&batch.deltas));
    engine.redo(agent.clone()).unwrap();
    let [redone] = drain(&events).try_into().unwrap();
    assert_eq!((redone.tx, redone.kind, &redone.origin, &redone.deltas), (batch.tx, ChangeKind::Redone, &agent, &batch.deltas));
}

/// The UI and the MCP server subscribe to the same `ChangeEvent` stream. A
/// subscriber on another thread sees every change, in order, and can rebuild the
/// project from the diffs alone; events survive a JSON round trip (the MCP wire).
#[test]
fn ui_and_mcp_consume_the_same_stream() {
    let engine = EngineHandle::new(Project::new("p"));
    let initial = engine.read(Project::clone);
    let ui = engine.subscribe();
    let mcp = engine.subscribe();

    let mcp_thread = std::thread::spawn(move || {
        let mut mirror = initial;
        let mut seen = Vec::new();
        for event in mcp {
            let wire = serde_json::to_string(&event).unwrap();
            let event: ChangeEvent = serde_json::from_str(&wire).unwrap();
            apply_all(&mut mirror, &event.deltas);
            seen.push(event);
        }
        (mirror, seen)
    });

    let agent = Origin::agent("mcp-client");
    let mut ids = Vec::new();
    for i in 0..50 {
        let origin = if i % 2 == 0 { Origin::User } else { agent.clone() };
        ids.push(engine.execute_typed(add(&format!("R{i}")), origin.clone()).unwrap().created[0]);
        engine.execute_typed(MovePart { id: ids[i / 2], x_mm: i as f64, y_mm: 0.0 }, origin).unwrap();
        if i % 7 == 0 {
            engine.undo(Origin::User).unwrap();
        }
    }
    let final_state = engine.read(Project::clone);
    drop(engine); // closes the channels, ending the MCP thread's loop

    let (mirror, mcp_events) = mcp_thread.join().unwrap();
    let ui_events = drain(&ui);
    assert_eq!(ui_events, mcp_events);
    assert_eq!(ui_events.len(), 100 + 8);
    assert_eq!(mirror, final_state, "the diffs alone rebuild the project");
}

#[test]
fn on_change_runs_after_each_change_outside_the_lock() {
    let engine = EngineHandle::new(Project::new("p"));
    let calls = Arc::new(AtomicUsize::new(0));
    let parts_seen = Arc::new(AtomicUsize::new(0));
    {
        let (calls, parts_seen, reader) = (calls.clone(), parts_seen.clone(), engine.clone());
        engine.on_change(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            // Calling back into the handle would deadlock if the lock were held.
            parts_seen.store(reader.read(|p| p.board.parts.len()), Ordering::SeqCst);
        });
    }

    engine.execute_typed(add("R1"), Origin::User).unwrap();
    assert_eq!((calls.load(Ordering::SeqCst), parts_seen.load(Ordering::SeqCst)), (1, 1));
    assert!(engine.execute_typed(add("R1"), Origin::User).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "no change, no wake");
    engine.undo(Origin::User).unwrap();
    assert_eq!((calls.load(Ordering::SeqCst), parts_seen.load(Ordering::SeqCst)), (2, 0));
    assert_eq!(engine.undo(Origin::User).unwrap(), None);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn dropped_subscribers_are_removed() {
    let engine = EngineHandle::new(Project::new("p"));
    let kept = engine.subscribe();
    drop(engine.subscribe());
    assert_eq!(engine.with_engine(Engine::subscriber_count), 2);
    engine.execute_typed(add("R1"), Origin::User).unwrap();
    assert_eq!(engine.with_engine(Engine::subscriber_count), 1);
    assert_eq!(drain(&kept).len(), 1);
}

#[test]
fn event_json_shape() {
    let engine = EngineHandle::new(Project::new("p"));
    let events = engine.subscribe();
    engine.execute_typed(add("R1"), Origin::agent("a")).unwrap();
    let json = serde_json::to_value(events.recv().unwrap()).unwrap();
    assert_eq!(json["tx"], 1);
    assert_eq!(json["origin"], json!({ "kind": "agent", "id": "a" }));
    assert_eq!(json["kind"], "applied");
    assert_eq!(json["deltas"][0]["kind"], "part");
    assert_eq!(json["deltas"][0]["after"]["designator"], "R1");
}
