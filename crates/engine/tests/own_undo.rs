//! Per-origin undo/redo: an agent undoes its own changes, not the user's.

use viamind_doc::{ObjectId, Project};
use viamind_engine::{AddPart, ChangeKind, DeletePart, EngineError, EngineHandle, MovePart, Origin};

fn add(designator: &str) -> AddPart {
    AddPart { designator: designator.into(), value: "1k".into(), footprint: "R_0603".into(), x_mm: 0.0, y_mm: 0.0, rotation_deg: None, side: None }
}

fn has(engine: &EngineHandle, id: ObjectId) -> bool {
    engine.read(|p| p.board.part(&id).is_some())
}

#[test]
fn agent_undo_leaves_user_changes_alone() {
    let engine = EngineHandle::new(Project::new("p"));
    let agent = Origin::agent("claude");
    let r1 = engine.execute_typed(add("R1"), agent.clone()).unwrap();
    let r2 = engine.execute_typed(add("R2"), Origin::User).unwrap().created[0];
    let events = engine.subscribe();

    assert_eq!(engine.next_undo_own(&agent).unwrap().id, r1.tx);
    assert_eq!(engine.undo_own(agent.clone()).unwrap(), Some(r1.tx));
    assert!(!has(&engine, r1.created[0]) && has(&engine, r2));
    let event = events.try_recv().unwrap();
    assert_eq!((event.tx, event.kind, &event.origin), (r1.tx, ChangeKind::Undone, &agent));

    assert_eq!(engine.undo_own(agent.clone()).unwrap(), None, "nothing else of the agent's to undo");
    assert_eq!(engine.next_redo_own(&agent).unwrap().id, r1.tx);
    assert_eq!(engine.redo_own(agent.clone()).unwrap(), Some(r1.tx));
    assert!(has(&engine, r1.created[0]) && has(&engine, r2));
    assert_eq!(engine.redo_own(agent).unwrap(), None);

    // The user's global undo still takes the latest change, whoever made it.
    assert_eq!(engine.undo(Origin::User).unwrap(), Some(r1.tx));
}

#[test]
fn conflicting_undo_is_refused_and_changes_nothing() {
    let engine = EngineHandle::new(Project::new("p"));
    let agent = Origin::agent("claude");
    let placed = engine.execute_typed(add("R1"), agent.clone()).unwrap();
    let id = placed.created[0];
    let moved = engine.execute_typed(MovePart { id, x_mm: 5.0, y_mm: 5.0 }, Origin::User).unwrap();
    let before = engine.read(Project::clone);
    let events = engine.subscribe();

    let err = engine.undo_own(agent.clone()).unwrap_err();
    assert_eq!(err, EngineError::UndoConflict { tx: placed.tx, blocked_by: moved.tx, blocked_by_origin: Origin::User });
    assert_eq!(err.to_string(), "cannot undo or redo tx1: tx2 by user changed the same objects; undo that first, or edit the objects directly");
    assert!(engine.read(|p| *p == before));
    assert!(events.try_recv().is_err(), "a refused undo sends no event");

    // Once the user's move is undone, the agent can undo its placement.
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.undo_own(agent).unwrap(), Some(placed.tx));
    assert!(engine.read(|p| p.board.parts.is_empty()));
}

#[test]
fn redo_own_waits_for_earlier_undone_changes() {
    let engine = EngineHandle::new(Project::new("p"));
    let (a, b) = (Origin::agent("a"), Origin::agent("b"));
    let placed = engine.execute_typed(add("R1"), a.clone()).unwrap();
    let moved = engine.execute_typed(MovePart { id: placed.created[0], x_mm: 1.0, y_mm: 1.0 }, b.clone()).unwrap();
    engine.undo(Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();

    // b's move needs a's placement redone first.
    let err = engine.redo_own(b.clone()).unwrap_err();
    assert!(matches!(err, EngineError::UndoConflict { tx, blocked_by, .. } if tx == moved.tx && blocked_by == placed.tx), "{err}");
    assert_eq!(engine.redo_own(a).unwrap(), Some(placed.tx));
    assert_eq!(engine.redo_own(b).unwrap(), Some(moved.tx));
    assert!(engine.read(|p| p.board.part(&placed.created[0]).unwrap().position == viamind_geom::Point::from_mm(1.0, 1.0)));
}

/// Small deterministic generator so the test needs no extra dependency.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

/// Random edits, global and per-origin undo/redo from three origins: the log
/// always replays to the live project, and undoing everything restores the start.
#[test]
fn mixed_undo_keeps_history_consistent() {
    let engine = EngineHandle::new(Project::new("p"));
    let initial = engine.read(Project::clone);
    let origins = [Origin::User, Origin::agent("a"), Origin::agent("b")];
    let mut rng = Lcg(99);
    let (mut refused, mut selective) = (0, 0);
    for step in 0..3_000 {
        let origin = origins[rng.below(3) as usize].clone();
        let ids: Vec<_> = engine.read(|p| p.board.parts.keys().copied().collect());
        let result = match rng.below(9) {
            _ if ids.is_empty() => engine.execute_typed(add(&format!("R{step}")), origin).map(drop),
            0 | 1 => engine.execute_typed(add(&format!("R{step}")), origin).map(drop),
            2 | 3 => {
                let id = ids[rng.below(ids.len() as u64) as usize];
                engine.execute_typed(MovePart { id, x_mm: f64::from(step), y_mm: 0.0 }, origin).map(drop)
            }
            4 => engine.execute_typed(DeletePart { id: ids[rng.below(ids.len() as u64) as usize] }, origin).map(drop),
            5 => engine.undo(origin).map(drop),
            6 => engine.redo(origin).map(drop),
            7 => engine.undo_own(origin).map(|tx| selective += usize::from(tx.is_some())),
            _ => engine.redo_own(origin).map(|tx| selective += usize::from(tx.is_some())),
        };
        if let Err(err) = result {
            assert!(matches!(err, EngineError::UndoConflict { .. }), "{err}");
            refused += 1;
        }
        if step % 100 == 0 {
            assert!(engine.with_engine(|e| e.log().replay() == *e.project()));
        }
    }
    assert!(selective > 100 && refused > 10, "exercised both paths: {selective} selective, {refused} refused");
    assert!(engine.with_engine(|e| e.log().replay() == *e.project()));
    while engine.undo(Origin::User).unwrap().is_some() {}
    assert!(engine.read(|p| *p == initial), "undoing everything restores the initial project");
}
