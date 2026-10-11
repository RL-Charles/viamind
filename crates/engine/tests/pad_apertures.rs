use viamind_doc::{ApertureRect, PadApertures, file, footprint::Footprint};
use viamind_engine::{ApplyFootprint, EngineHandle, EventLog, Origin, PadInput, SetPartPads};
use viamind_geom::Nm;
fn engine() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap())
}
fn downgrade(log: &EventLog) -> Vec<u8> {
    let mut bytes = Vec::new();
    log.write_jsonl(&mut bytes).unwrap();
    String::from_utf8(bytes).unwrap().replacen(&format!("\"version\":{}", viamind_engine::log::LOG_VERSION), "\"version\":13", 1).into_bytes()
}
#[test]
fn source_policy_identity_net_retention_full_cursor_and_both_log_delta_sides() {
    let e = engine();
    let before = e.session_state();
    let part = before.project.board.parts.values().next().unwrap().clone();
    let legacy = Footprint::from_part(&part).unwrap();
    let mut definition = legacy.clone();
    definition.pads[0].apertures = Some(PadApertures { mask: Some(ApertureRect { width: Nm(3), height: Nm(5) }), paste: None });
    let mut edit = ApplyFootprint { expected_project: before.project.id, expected_cursor: None, expected: part.clone(), footprint: definition };
    assert!(e.execute_typed(edit.clone(), Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    edit.expected_cursor = Some(before.cursor);
    e.execute_typed(edit, Origin::User).unwrap();
    let after = e.session_state();
    let changed = &after.project.board.parts[&part.id];
    let mut expected = part.clone();
    expected.pads.as_mut().unwrap().0[0].apertures = changed.pads.as_ref().unwrap().0[0].apertures.clone();
    assert_eq!(changed, &expected);
    assert_eq!(file::minimum_version(&after.project), 14);
    assert!(e.with_engine(|eng| EventLog::read_jsonl(downgrade(eng.log()).as_slice()).is_err()));
    assert!(EventLog::read_jsonl(downgrade(&EventLog::new(after.project.clone())).as_slice()).is_err());
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
    let cursor = e.session_state().cursor;
    e.execute_typed(
        ApplyFootprint { expected_project: before.project.id, expected_cursor: Some(cursor), expected: changed.clone(), footprint: legacy },
        Origin::User,
    )
    .unwrap();
    let event = e.with_engine(|eng| eng.log().entries().last().unwrap().clone());
    let mut before_only = EventLog::new(before.project.clone());
    before_only.append(event);
    assert!(EventLog::read_jsonl(downgrade(&before_only).as_slice()).is_err());
    assert_eq!(e.read(Clone::clone), before.project);
}
#[test]
fn copper_resize_retains_absolute_policy_and_invalid_moved_extents_are_atomic() {
    let e = engine();
    let baseline = e.session_state();
    let part = baseline.project.board.parts.values().next().unwrap().clone();
    let mut f = Footprint::from_part(&part).unwrap();
    f.pads[0].apertures = Some(PadApertures { mask: Some(ApertureRect { width: Nm(3), height: Nm(5) }), paste: None });
    e.execute_typed(
        ApplyFootprint { expected_project: baseline.project.id, expected_cursor: Some(baseline.cursor), expected: part, footprint: f },
        Origin::User,
    )
    .unwrap();
    let before = e.session_state();
    let part = before.project.board.parts.values().next().unwrap();
    let mut inputs: Vec<_> = part
        .pads
        .as_ref()
        .unwrap()
        .0
        .iter()
        .map(|p| PadInput {
            id: Some(p.id),
            number: p.number.clone(),
            x_mm: p.position.x.to_mm(),
            y_mm: p.position.y.to_mm(),
            width_mm: p.width.to_mm(),
            height_mm: p.height.to_mm(),
            net: p.net.clone(),
        })
        .collect();
    inputs[0].width_mm = 0.1;
    inputs[0].height_mm = 0.2;
    e.execute_typed(SetPartPads { id: part.id, expected: part.clone(), pads: inputs.clone() }, Origin::User).unwrap();
    let after = e.session_state();
    let changed = &after.project.board.parts[&part.id];
    assert_eq!(changed.pads.as_ref().unwrap().0[0].apertures, part.pads.as_ref().unwrap().0[0].apertures);
    inputs[0].x_mm = 10000.0;
    assert!(e.execute_typed(SetPartPads { id: part.id, expected: changed.clone(), pads: inputs }, Origin::User).is_err());
    assert_eq!(e.session_state(), after);
}
