use viamind_doc::{FootprintHole, ObjectId, PartHole, file, footprint::Footprint};
use viamind_engine::{ApplyFootprint, CommandCall, EngineHandle, Origin, SetPartHoles};
use viamind_geom::{Nm, Point};
fn engine() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap())
}
fn hole(key: &str) -> PartHole {
    PartHole { id: ObjectId::new(), key: key.into(), center: Point::new(Nm(1_000_003), Nm(-2_000_007)), diameter: Nm(3_000_001) }
}
fn request(e: &EngineHandle, holes: Option<Vec<PartHole>>) -> SetPartHoles {
    let s = e.session_state();
    SetPartHoles {
        expected_project: s.project.id,
        expected_cursor: s.cursor,
        expected: s.project.board.parts.values().next().unwrap().clone(),
        holes,
    }
}
#[test]
fn guarded_atomic_edit_preserves_whole_project_and_nullable_intent_undo_redo_noop() {
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let r = request(&e, Some(vec![hole("mount.A")]));
    let mut expected = before.project.clone();
    expected.board.parts.get_mut(&r.expected.id).unwrap().holes = r.holes.clone();
    e.execute_typed(r.clone(), Origin::User).unwrap();
    let after = e.session_state();
    assert_eq!(after.project, expected);
    assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
    assert_eq!(file::minimum_version(&after.project), 15);
    assert!(e.execute_typed(r.clone(), Origin::User).is_err());
    assert_eq!(e.session_state(), after);
    e.execute_typed(request(&e, r.holes.clone()), Origin::User).unwrap();
    assert_eq!(e.session_state(), after);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), expected);
    e.execute_typed(request(&e, Some(vec![])), Origin::User).unwrap();
    assert_eq!(file::minimum_version(&e.read(Clone::clone)), 15);
    e.execute_typed(request(&e, None), Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    assert_eq!(e.document_status().path, status.path);
}
#[test]
fn raw_required_null_duplicate_poison_and_geometry_refusals_are_atomic() {
    let e = engine();
    let before = e.session_state();
    let r = request(&e, None);
    let raw = serde_json::to_value(&r).unwrap();
    let mut missing = raw.clone();
    missing.as_object_mut().unwrap().remove("holes");
    assert!(serde_json::from_value::<SetPartHoles>(missing).unwrap_err().to_string().contains("missing field"));
    assert!(serde_json::from_value::<SetPartHoles>(raw.clone()).is_ok());
    let mut poison = raw.clone();
    poison["holes"] = serde_json::json!(vec![serde_json::Value::Null; 257]);
    assert!(serde_json::from_value::<SetPartHoles>(poison).unwrap_err().to_string().contains("256"));
    let mut bad = r.clone();
    let mut h = hole("ok");
    h.diameter = Nm(0);
    bad.holes = Some(vec![h]);
    assert!(e.execute_typed(bad, Origin::User).is_err());
    let mut duplicate = r.clone();
    duplicate.holes = Some(vec![hole("same"), hole("same")]);
    assert!(e.execute_typed(duplicate, Origin::User).is_err());
    let text = serde_json::to_string(&request(&e, Some(vec![hole("a")]))).unwrap();
    let dup = text.replace("\"key\":\"a\"", "\"key\":\"a\",\"key\":\"b\"");
    assert!(serde_json::from_str::<SetPartHoles>(&dup).unwrap_err().to_string().contains("duplicate"));
    assert_eq!(e.session_state(), before);
}
#[test]
fn complete_global_namespace_rejects_foreign_pad_part_project_and_retains_owned_id() {
    let e = engine();
    let before = e.session_state();
    let r = request(&e, Some(vec![hole("first")]));
    let foreign =
        before.project.board.parts.values().filter(|p| p.id != r.expected.id).flat_map(|p| p.pads.iter().flat_map(|p| &p.0)).next().unwrap().id;
    for id in [before.project.id, r.expected.id, foreign, r.expected.pads.as_ref().unwrap().0[0].id] {
        let mut h = hole("new");
        h.id = id;
        assert!(e.execute_typed(request(&e, Some(vec![h])), Origin::User).is_err());
        assert_eq!(e.session_state(), before);
    }
    e.execute_typed(r, Origin::User).unwrap();
    let s = e.session_state();
    let mut retained = s.project.board.parts.values().next().unwrap().holes.clone().unwrap();
    retained[0].key = "renamed".into();
    e.execute_typed(request(&e, Some(retained.clone())), Origin::User).unwrap();
    assert_eq!(e.session_state().project.board.parts.values().next().unwrap().holes.as_ref().unwrap(), &retained);
}
#[test]
fn keyed_detached_reuse_retains_pad_nets_hole_ids_and_old_definition_restores_legacy() {
    let e = engine();
    let base = e.session_state();
    let p = base.project.board.parts.values().next().unwrap().clone();
    let legacy = Footprint::from_part(&p).unwrap();
    let mut f = legacy.clone();
    f.holes = Some(vec![FootprintHole { key: "A".into(), center: Point::ORIGIN, diameter: Nm(1_000_001) }]);
    let apply = |e: &EngineHandle, f: Footprint, cursor: bool| {
        let s = e.session_state();
        e.execute_typed(
            ApplyFootprint {
                expected_project: s.project.id,
                expected_cursor: cursor.then_some(s.cursor),
                expected: s.project.board.parts[&p.id].clone(),
                footprint: f,
            },
            Origin::User,
        )
    };
    assert!(apply(&e, f.clone(), false).is_err());
    assert_eq!(e.session_state(), base);
    apply(&e, f.clone(), true).unwrap();
    let first = e.session_state();
    let id = first.project.board.parts[&p.id].holes.as_ref().unwrap()[0].id;
    assert_eq!(first.project.board.parts[&p.id].pads, p.pads);
    f.holes.as_mut().unwrap()[0].diameter = Nm(2_000_003);
    f.holes.as_mut().unwrap().push(FootprintHole { key: "B".into(), center: Point::new(Nm(4_000_000), Nm(0)), diameter: Nm(1_000_001) });
    apply(&e, f, true).unwrap();
    let second = e.session_state();
    let h = second.project.board.parts[&p.id].holes.as_ref().unwrap();
    assert_eq!(h[0].id, id);
    assert_ne!(h[1].id, id);
    file::validate_holes(&second.project).unwrap();
    assert_eq!(second.project.board.parts[&p.id].pads, p.pads);
    assert!(apply(&e, legacy.clone(), false).is_err());
    apply(&e, legacy, true).unwrap();
    assert_eq!(e.read(Clone::clone), base.project);
}
#[test]
fn batch_initial_cursor_rolls_back_all_steps_and_peer_edit_invalidates_snapshot() {
    let e = engine();
    let before = e.session_state();
    let first = request(&e, Some(vec![hole("A")]));
    let mut expected = first.expected.clone();
    expected.holes = first.holes.clone();
    let second = SetPartHoles { expected, holes: Some(vec![]), ..first.clone() };
    e.execute_batch(
        vec![
            CommandCall::new("set_part_holes", serde_json::to_value(first.clone()).unwrap()),
            CommandCall::new("set_part_holes", serde_json::to_value(second).unwrap()),
        ],
        Origin::User,
        None,
    )
    .unwrap();
    let after = e.session_state();
    assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
    assert_eq!(after.project.board.parts[&first.expected.id].holes, Some(vec![]));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    let stale = request(&e, None);
    let mut bad = stale.clone();
    bad.expected_project = ObjectId::new();
    assert!(
        e.execute_batch(
            vec![
                CommandCall::new("set_part_holes", serde_json::to_value(request(&e, Some(vec![hole("x")]))).unwrap()),
                CommandCall::new("set_part_holes", serde_json::to_value(bad).unwrap())
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.read(Clone::clone), before.project);
    assert!(e.execute_typed(first, Origin::User).is_err());
}
#[test]
fn hole_identities_share_the_explicit_bus_and_entry_namespace() {
    let mut project = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let buses = file::from_str(include_str!("../../../fixtures/schematic/buses.viamind.json")).unwrap();
    project.schematic.sheets.extend(buses.schematic.sheets);
    let e = EngineHandle::new(project);
    let s = e.session_state();
    let unfold: viamind_engine::UnfoldSchematicBusMember = serde_json::from_value(serde_json::json!({
        "source": {"expected_project": s.project.id, "source_cursor": s.cursor, "sheet_id": "00000000-0000-0000-0000-000000001271"},
        "bus_id": "00000000-0000-0000-0000-000000001272", "member": "DATA0",
        "bus_point": {"x": 0, "y": 0}, "wire_point": {"x": 2000000, "y": 2000000}, "wire_end": {"x": 2000000, "y": 8000000}
    }))
    .unwrap();
    e.execute_typed(unfold, Origin::User).unwrap();
    let before = e.session_state();
    let content = before.project.schematic.sheets.values().find_map(|s| s.content.as_ref().filter(|c| !c.buses.is_empty())).unwrap();
    let entry = content.bus_entries.values().next().unwrap();
    for id in [*content.buses.keys().next().unwrap(), entry.id, entry.wire_id, entry.label_id] {
        let mut h = hole("bus-collision");
        h.id = id;
        let refused = e.execute_typed(request(&e, Some(vec![h])), Origin::User).unwrap_err();
        assert!(refused.to_string().contains("collides"), "{id}: {refused}");
        assert_eq!(e.session_state(), before);
    }
    e.execute_typed(request(&e, Some(vec![hole("distinct")])), Origin::User).unwrap();
    assert_eq!(file::minimum_version(&e.read(Clone::clone)), 16);
}
