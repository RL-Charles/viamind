use serde_json::json;
use viamind_doc::{FootprintArtwork, Project, SilkscreenSegment};
use viamind_engine::{CommandCall, EngineHandle, EventLog, Origin, SetFootprintArtwork};
use viamind_geom::{Nm, Point};

fn engine() -> EngineHandle {
    let e = EngineHandle::new(Project::new("Artwork"));
    e.execute("add_part", json!({"designator":"U1","value":"IC","footprint":"Explicit","x_mm":1,"y_mm":2}), Origin::User).unwrap();
    e
}
fn command(e: &EngineHandle, artwork: Option<Box<FootprintArtwork>>) -> SetFootprintArtwork {
    e.with_engine(|eng| SetFootprintArtwork {
        expected_project: eng.project().id,
        expected_cursor: eng.event_cursor(),
        expected: eng.project().board.parts.values().next().unwrap().clone(),
        artwork,
    })
}
fn artwork() -> Box<FootprintArtwork> {
    Box::new(FootprintArtwork {
        silkscreen: vec![SilkscreenSegment { start: Point::new(Nm(-3), Nm(5)), end: Point::new(Nm(7), Nm(11)), width: Nm(3) }],
        courtyard: None,
    })
}
#[test]
fn guarded_atomic_artwork_noop_undo_log_and_reopen() {
    let e = engine();
    let before = e.session_state();
    let edit = command(&e, Some(artwork()));
    let result = e.execute_typed(edit.clone(), Origin::agent("art")).unwrap();
    assert_eq!(result.deltas.len(), 1);
    let after = e.session_state();
    let mut expected = edit.expected.clone();
    expected.artwork = edit.artwork.clone();
    assert_eq!(after.project.board.parts[&expected.id], expected);
    assert!(e.execute_typed(edit, Origin::User).is_err());
    assert_eq!(e.session_state(), after);
    assert!(e.execute_typed(command(&e, Some(artwork())), Origin::User).unwrap().deltas.is_empty());
    assert_eq!(e.session_state(), after);
    let mut log = Vec::new();
    e.with_engine(|eng| eng.log().write_jsonl(&mut log)).unwrap();
    assert_eq!(EventLog::read_jsonl(log.as_slice()).unwrap().replay(), after.project);
    let text = String::from_utf8(log).unwrap();
    assert!(EventLog::read_jsonl(text.replace("\"version\":13", "\"version\":12").as_bytes()).is_err());
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
    let stale = command(&e, None);
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    let reopened = e.session_state();
    assert!(e.execute_typed(stale, Origin::User).is_err());
    assert_eq!(e.session_state(), reopened);
}
#[test]
fn batch_uses_one_initial_cursor_and_raw_overflow_precedes_poison() {
    let e = engine();
    let before = e.session_state();
    let first = command(&e, Some(artwork()));
    let mut second = first.clone();
    second.expected.artwork = first.artwork.clone();
    second.artwork = Some(Box::default());
    e.execute_batch(
        vec![
            CommandCall::new("set_footprint_artwork", serde_json::to_value(first).unwrap()),
            CommandCall::new("set_footprint_artwork", serde_json::to_value(second).unwrap()),
        ],
        Origin::User,
        None,
    )
    .unwrap();
    assert!(e.read(|p| p.board.parts.values().next().unwrap().artwork.as_ref().unwrap().silkscreen.is_empty()));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    let baseline = e.session_state();
    let mut input = serde_json::to_value(command(&e, None)).unwrap();
    input["artwork"] = json!({"silkscreen":vec![json!("poison");257]});
    let error = e.execute("set_footprint_artwork", input, Origin::User).unwrap_err();
    assert!(error.to_string().contains("at most256"), "{error}");
    assert_eq!(e.session_state(), baseline);
}
