use serde_json::json;
use viamind_doc::{
    ObjectId, Project, Side,
    footprint::{Footprint, FootprintPad, LocalPosition},
};
use viamind_engine::{ApplyFootprint, EngineHandle, EventLog, Origin};
use viamind_geom::Nm;

#[test]
fn reuse_preserves_matching_pad_ids_nets_and_placement_with_one_undo() {
    let engine = EngineHandle::new(Project::new("Reuse"));
    let id = engine
        .execute("add_part", json!({"designator":"U1","value":"IC","footprint":"Old","x_mm":12.345678,"y_mm":-20}), Origin::User)
        .unwrap()
        .created[0];
    let initial = engine.read(|p| p.board.part(&id).unwrap().clone());
    engine
        .execute(
            "update_part",
            json!({"id":id,"expected":initial,"designator":"U1","value":"IC","footprint":"Old","rotation_deg":45,"side":"bottom"}),
            Origin::User,
        )
        .unwrap();
    let part = engine.read(|p| p.board.part(&id).unwrap().clone());
    engine.execute("set_part_pads",json!({"id":id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":"GND"},{"number":"gone","x_mm":2,"y_mm":0,"width_mm":1,"height_mm":1,"net":"VCC"}]}),Origin::User).unwrap();
    let before = engine.read(Clone::clone);
    let part = before.board.part(&id).unwrap();
    assert_eq!(part.side, Side::Bottom);
    let definition = Footprint {
        holes: None,
        artwork: None,
        name: "New_SMD".into(),
        pads: ["2", "1"]
            .into_iter()
            .map(|number| FootprintPad {
                number: number.into(),
                position: LocalPosition { x: Nm(-800001), y: Nm(123) },
                width: Nm(900003),
                height: Nm(950007),
                apertures: None,
            })
            .collect(),
    };
    let command = ApplyFootprint { expected_cursor: None, expected_project: before.id, expected: part.clone(), footprint: definition };
    let result = engine.execute_typed(command.clone(), Origin::agent("library")).unwrap();
    assert_eq!(result.deltas.len(), 1);
    let after = engine.read(Clone::clone);
    let changed = after.board.part(&id).unwrap();
    assert_eq!(
        (changed.position, changed.rotation_mdeg, changed.side, changed.designator.as_str(), changed.value.as_str()),
        (part.position, part.rotation_mdeg, part.side, part.designator.as_str(), part.value.as_str())
    );
    let pads = &changed.pads.as_ref().unwrap().0;
    assert_ne!(pads[0].id, pads[1].id);
    assert!(pads[0].net.is_none());
    assert_eq!(pads[1].id, part.pads.as_ref().unwrap().0[0].id);
    assert_eq!(pads[1].net.as_deref(), Some("GND"));
    assert_eq!(pads[1].position.x.0, -800001);
    assert_eq!(Footprint::from_part(changed).unwrap(), command.footprint);
    let state = engine.session_state();
    let mut noop = command.clone();
    noop.expected = changed.clone();
    assert!(engine.execute_typed(noop.clone(), Origin::User).unwrap().deltas.is_empty());
    assert_eq!(engine.session_state(), state);
    let mut invalid = noop.clone();
    invalid.footprint.pads[0].width = Nm(0);
    assert!(engine.execute_typed(invalid, Origin::User).is_err());
    let mut invalid = noop;
    invalid.footprint.pads[1].number = "2".into();
    assert!(engine.execute_typed(invalid, Origin::User).is_err());
    assert!(engine.execute_typed(command.clone(), Origin::User).is_err());
    let mut bad = command.clone();
    bad.expected = changed.clone();
    bad.expected_project = ObjectId::new();
    assert!(engine.execute_typed(bad, Origin::User).is_err());
    assert_eq!(engine.session_state(), state);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
    let mut bytes = Vec::new();
    engine.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
    assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), after);
    assert_eq!(viamind_doc::file::from_str(&viamind_doc::file::to_string(&after)).unwrap(), after);
    let empty = ApplyFootprint {
        expected_cursor: None,
        expected_project: after.id,
        expected: changed.clone(),
        footprint: Footprint { holes: None, artwork: None, name: "Empty".into(), pads: vec![] },
    };
    engine.execute_typed(empty, Origin::User).unwrap();
    assert!(engine.read(|p| p.board.part(&id).unwrap().pads.as_ref().unwrap().0.is_empty()));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), after);
}
