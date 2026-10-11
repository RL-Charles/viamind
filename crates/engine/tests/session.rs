//! Observable session behavior: ordering, bounded replay, concurrent observers and silence.
use std::{sync::mpsc, thread, time::Instant};

use serde_json::json;
use viamind_doc::{ObjectId, Project};
use viamind_engine::{
    ChangeKind, EngineHandle, Origin, SESSION_EVENT_CAPACITY, SelectionMode, SessionError, SessionEvent, SetSelection, WatchEvents,
};

fn engine() -> EngineHandle {
    EngineHandle::new(Project::new("session"))
}

fn add(engine: &EngineHandle) -> ObjectId {
    engine
        .execute("add_part", json!({ "designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 0, "y_mm": 0 }), Origin::agent("writer"))
        .unwrap()
        .created[0]
}

#[test]
fn snapshot_replay_orders_design_selection_and_pruning_with_origins() {
    let engine = engine();
    let baseline = engine.session_state();
    let documents = engine.subscribe();
    let id = add(&engine);
    engine.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Replace }, Origin::agent("selector")).unwrap();
    engine.execute("delete_part", json!({ "id": id }), Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    engine.redo(Origin::User).unwrap();
    let input = WatchEvents { after: Some(baseline.cursor), ..Default::default() };
    let batch = engine.watch_events(input.clone()).unwrap();
    assert_eq!(batch, engine.watch_events(input).unwrap(), "replaying a cursor is deterministic");
    assert_eq!(batch.events.iter().map(|event| event.sequence).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
    assert!(!batch.has_more);
    assert!(matches!(&batch.events[0].event, SessionEvent::Design(event) if event.origin == Origin::agent("writer")));
    assert!(
        matches!(&batch.events[1].event, SessionEvent::Selection(selection) if selection.ids == [id] && selection.origin == Some(Origin::agent("selector")))
    );
    assert!(matches!(&batch.events[2].event, SessionEvent::Design(event) if event.kind == ChangeKind::Applied && event.origin == Origin::User));
    assert!(
        matches!(&batch.events[3].event, SessionEvent::Selection(selection) if selection.ids.is_empty() && selection.revision == 2 && selection.origin == Some(Origin::User))
    );
    assert!(matches!(&batch.events[4].event, SessionEvent::Design(event) if event.kind == ChangeKind::Undone));
    assert!(matches!(&batch.events[5].event, SessionEvent::Design(event) if event.kind == ChangeKind::Redone));
    let snapshot = engine.session_state();
    assert_eq!(snapshot.project, baseline.project);
    assert!(snapshot.selection.ids.is_empty());
    assert_eq!(snapshot.cursor, batch.cursor);
    assert_eq!(documents.try_iter().count(), 4, "selection stays out of the existing document channel");
    assert_eq!(engine.with_engine(|engine| engine.log().len()), 4);
}

#[test]
fn reads_noops_errors_and_selection_preserve_history_and_redo() {
    let engine = engine();
    let id = add(&engine);
    engine.execute("move_part", json!({ "id": id, "x_mm": 1, "y_mm": 0 }), Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    let before = engine.session_state();
    let log_len = engine.with_engine(|engine| engine.log().len());
    engine.query_parts(&Default::default()).unwrap();
    engine.execute("move_part", json!({ "id": id, "x_mm": 0, "y_mm": 0 }), Origin::User).unwrap();
    assert!(engine.execute("delete_part", json!({ "id": ObjectId::new() }), Origin::User).is_err());
    assert!(engine.set_selection(SetSelection { ids: vec![ObjectId::new()], mode: SelectionMode::Replace }, Origin::User).is_err());
    engine.set_selection(SetSelection { ids: vec![], mode: SelectionMode::Replace }, Origin::User).unwrap();
    assert_eq!(engine.session_state(), before);
    assert!(engine.watch_events(WatchEvents { after: Some(before.cursor), ..Default::default() }).unwrap().events.is_empty());
    engine.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Replace }, Origin::User).unwrap();
    let selected = engine.session_state();
    engine.set_selection(SetSelection { ids: vec![id, id], mode: SelectionMode::Add }, Origin::agent("noop")).unwrap();
    assert_eq!(engine.session_state(), selected);
    assert!(engine.can_redo());
    assert_eq!(engine.with_engine(|engine| engine.log().len()), log_len);
    assert_eq!(engine.with_engine(|engine| engine.undo_history().len()), 1);
}

#[test]
fn bounded_history_pages_and_cursor_errors_allow_resynchronization() {
    let engine = engine();
    let original = engine.session_state().cursor;
    let id = add(&engine);
    for _ in 0..SESSION_EVENT_CAPACITY {
        engine.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Toggle }, Origin::User).unwrap();
    }
    let read = |after, limit| engine.watch_events(WatchEvents { after: Some(after), limit, ..Default::default() });
    assert_eq!(read(original, 100), Err(SessionError::ExpiredCursor));
    let oldest_valid = viamind_engine::EventCursor { sequence: 1, ..original };
    let mut cursor = oldest_valid;
    let mut sequences = Vec::new();
    loop {
        let batch = read(cursor, 73).unwrap();
        sequences.extend(batch.events.iter().map(|event| event.sequence));
        cursor = batch.cursor;
        if !batch.has_more {
            break;
        }
    }
    assert_eq!(sequences, (2..=(SESSION_EVENT_CAPACITY as u64 + 1)).collect::<Vec<_>>());
    assert_eq!(cursor, engine.session_state().cursor);
    assert_eq!(read(viamind_engine::EventCursor { sequence: cursor.sequence + 1, ..cursor }, 1), Err(SessionError::FutureCursor));
    let restarted = EngineHandle::new(engine.read(Clone::clone));
    assert_eq!(restarted.watch_events(WatchEvents { after: Some(cursor), ..Default::default() }), Err(SessionError::WrongSession));
    let fresh = restarted.session_state();
    assert_eq!(fresh.cursor.sequence, 0);
    assert!(restarted.watch_events(WatchEvents { after: Some(fresh.cursor), ..Default::default() }).unwrap().events.is_empty());
    assert!(read(cursor, 0).is_err());
    assert!(read(cursor, 1001).is_err());
    assert!(engine.watch_events(WatchEvents { timeout_ms: 30001, ..Default::default() }).is_err());
    assert!(serde_json::from_value::<WatchEvents>(json!({ "unknown": true })).is_err());
    assert!(serde_json::from_value::<WatchEvents>(json!({ "after": { "stream_id": original.stream_id, "sequence": 0, "extra": true } })).is_err());
}

#[test]
fn independent_waiters_wake_for_design_and_selection_without_holding_the_lock() {
    let engine = engine();
    let baseline = engine.session_state().cursor;
    // Reentrant repaint callbacks must remain outside the engine lock.
    let callback_handle = engine.clone();
    let (called, calls) = mpsc::channel();
    engine.on_change(move || {
        called.send(callback_handle.session_state().cursor).unwrap();
    });
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            let engine = engine.clone();
            thread::spawn(move || engine.watch_events(WatchEvents { after: Some(baseline), timeout_ms: 2000, ..Default::default() }).unwrap())
        })
        .collect();
    let id = add(&engine);
    for waiter in waiters {
        assert_eq!(waiter.join().unwrap().events.len(), 1);
    }
    let before_selection = engine.session_state().cursor;
    let observer = engine.clone();
    let waiter =
        thread::spawn(move || observer.watch_events(WatchEvents { after: Some(before_selection), timeout_ms: 2000, ..Default::default() }).unwrap());
    engine.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Replace }, Origin::User).unwrap();
    assert!(matches!(waiter.join().unwrap().events[0].event, SessionEvent::Selection(_)));
    assert_eq!(calls.try_iter().count(), 2);
}

#[test]
fn absent_cursor_starts_at_tail_and_timeout_returns_unchanged_cursor() {
    let engine = engine();
    add(&engine);
    let tail = engine.session_state().cursor;
    let start = Instant::now();
    let batch = engine.watch_events(WatchEvents { timeout_ms: 20, ..Default::default() }).unwrap();
    assert!(start.elapsed().as_millis() >= 20);
    assert_eq!(batch.cursor, tail);
    assert!(batch.events.is_empty());
    assert!(!batch.has_more);
}
