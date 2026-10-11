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
    // A base-only artwork feature and a before-only deletion feature must each
    // refuse old headers independently of an after value.
    let base_only = EventLog::new(after.project.clone());
    let mut base_bytes = Vec::new();
    base_only.write_jsonl(&mut base_bytes).unwrap();
    assert!(EventLog::read_jsonl(String::from_utf8(base_bytes).unwrap().replace("\"version\":13", "\"version\":12").as_bytes()).is_err());
    let before_only = EngineHandle::new(after.project.clone());
    before_only.execute_typed(command(&before_only, None), Origin::User).unwrap();
    let event = before_only.with_engine(|eng| eng.log().entries()[0].clone());
    let mut isolated = EventLog::new(before.project.clone());
    isolated.append(event);
    let mut isolated_bytes = Vec::new();
    isolated.write_jsonl(&mut isolated_bytes).unwrap();
    assert!(EventLog::read_jsonl(String::from_utf8(isolated_bytes).unwrap().replace("\"version\":13", "\"version\":12").as_bytes()).is_err());

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

#[test]
fn library_two_apply_export_native_save_pad_net_identity_and_assembly_delta_retention() {
    use viamind_doc::{AssemblyMetadata, file, footprint::Footprint};
    use viamind_engine::{ApplyFootprint, FootprintFile, SaveProject, SetPartAssembly};
    let e = EngineHandle::new(file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap());
    let id = e.read(|p| *p.board.parts.keys().next().unwrap());
    let before = e.session_state();
    let part = before.project.board.parts[&id].clone();
    let mut definition = Footprint::from_part(&part).unwrap();
    definition.artwork = Some(artwork());
    definition.name = "Explicit authored".into();
    let mut apply =
        ApplyFootprint { expected_project: before.project.id, expected_cursor: None, expected: part.clone(), footprint: definition.clone() };
    assert!(e.execute_typed(apply.clone(), Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    apply.expected_cursor = Some(before.cursor);
    e.execute("add_part", json!({"designator":"PEER","value":"peer","footprint":"test","x_mm":1,"y_mm":2}), Origin::agent("peer")).unwrap();
    let peer = e.session_state();
    assert!(e.execute_typed(apply.clone(), Origin::User).is_err());
    assert_eq!(e.session_state(), peer);
    apply.expected_cursor = Some(peer.cursor);
    e.execute_typed(apply, Origin::User).unwrap();
    let changed = e.read(|p| p.board.parts[&id].clone());
    assert_eq!(changed.pads, part.pads);
    assert_eq!(changed.position, part.position);
    assert_eq!(changed.side, part.side);
    assert_eq!(changed.artwork, definition.artwork);
    let baseline = e.session_state();
    e.execute_typed(
        SetPartAssembly {
            id,
            assembly: Some(AssemblyMetadata { mpn: Some("Exact user field".into()), ..Default::default() }),
            expected: Some(changed.clone()),
            expected_project: Some(before.project.id),
        },
        Origin::User,
    )
    .unwrap();
    assert_eq!(e.read(|p| p.board.parts[&id].artwork.clone()), changed.artwork, "ordinary clone_without_assembly must retain live artwork");
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), baseline.project);
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("explicit.footprint.json");
    e.export_footprint(FootprintFile {
        expected_cursor: Some(e.event_cursor()),
        expected_project: before.project.id,
        expected: changed.clone(),
        path: library.clone(),
    })
    .unwrap();
    assert_eq!(viamind_doc::footprint::load(&library).unwrap(), definition);
    let native = temp.path().join("explicit.viamind.json");
    e.save_project(SaveProject { path: Some(native.clone()), overwrite: false }).unwrap();
    assert_eq!(file::load(&native).unwrap(), e.read(Clone::clone));
    let restored = ApplyFootprint {
        expected_project: before.project.id,
        expected_cursor: Some(e.event_cursor()),
        expected: changed,
        footprint: Footprint::from_part(&part).unwrap(),
    };
    e.execute_typed(restored, Origin::User).unwrap();
    assert!(e.read(|p| p.board.parts[&id].artwork.is_none()));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), baseline.project);
}
