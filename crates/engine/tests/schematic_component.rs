use serde_json::json;
use viamind_doc::{
    ObjectId, Side, file,
    footprint::{Footprint, FootprintPad, LocalPosition},
};
use viamind_engine::{
    Command, CommandCall, CreateSchematicPcbComponent, EngineHandle, EventLog, Origin, SchematicComponentPlacement, SchematicComponentRequest,
};
use viamind_geom::{Nm, Point};
fn fixture() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
}
fn request(e: &EngineHandle, reference: &str) -> SchematicComponentRequest {
    let (project, source_cursor) = e.schematic_pcb_snapshot().unwrap();
    let symbol = project
        .schematic
        .sheets
        .values()
        .flat_map(|s| s.content.iter().flat_map(|c| c.symbols.values()))
        .find(|s| s.designator == reference)
        .unwrap();
    SchematicComponentRequest {
        expected_project: project.id,
        source_cursor,
        placement: SchematicComponentPlacement {
            symbol_id: symbol.id,
            position: Point::new(Nm(40_000_001), Nm(30_000_003)),
            side: Side::Bottom,
            rotation_mdeg: 90_000,
            footprint: Footprint {
                name: "Original SMD".into(),
                pads: symbol
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(i, p)| FootprintPad {
                        number: p.number.clone(),
                        position: LocalPosition { x: Nm(i as i64 * 2_000_000), y: Nm(257) },
                        width: Nm(800_001),
                        height: Nm(600_003),
                    })
                    .collect(),
            },
        },
    }
}
#[test]
fn typed_dynamic_creation_exact_independent_regulator_groups_persistence_and_one_undo() {
    for typed in [true, false] {
        let e = fixture();
        e.execute(
            "add_track",
            json!({"start_x_mm":1,"start_y_mm":1,"end_x_mm":3,"end_y_mm":1,"width_mm":0.25,"layer":"top","net":"EXISTING"}),
            Origin::User,
        )
        .unwrap();
        let before = e.session_state();
        for reference in ["U1", "C1", "C2"] {
            let checkpoint = e.session_state();
            let req = request(&e, reference);
            let preview = e.preview_schematic_pcb_component(req.clone()).unwrap();
            assert_eq!(e.session_state(), checkpoint);
            assert_eq!(
                serde_json::to_value(e.preview_schematic_pcb_component(req.clone()).unwrap()).unwrap(),
                serde_json::to_value(&preview).unwrap(),
                "preview publishes no random new identities"
            );
            let result = if typed {
                e.execute_typed(preview.command, Origin::User)
            } else {
                e.execute(CreateSchematicPcbComponent::NAME, serde_json::to_value(preview.command).unwrap(), Origin::agent("creator"))
            }
            .unwrap();
            assert_eq!(result.created.len(), 1);
            assert_eq!(e.event_cursor().sequence, checkpoint.cursor.sequence + 1);
            let after = e.read(Clone::clone);
            let part = &after.board.parts[&result.created[0]];
            assert_eq!((part.position, part.side, part.rotation_mdeg), (req.placement.position, Side::Bottom, 90_000));
            assert_eq!(part.schematic_symbol, Some(req.placement.symbol_id));
            assert_eq!(part.designator, reference);
            assert_eq!(part.pads.as_ref().unwrap().0.len(), req.placement.footprint.pads.len());
            let mut normalized = after.clone();
            normalized.board.parts.remove(&part.id);
            assert_eq!(normalized, checkpoint.project, "only one Part added");
            e.undo(Origin::User).unwrap();
            assert_eq!(e.read(Clone::clone), checkpoint.project);
            e.redo(Origin::User).unwrap();
            assert_eq!(e.read(Clone::clone), after, "redo preserves all UUIDs");
        }
        let after = e.read(Clone::clone);
        assert_eq!(after.schematic, before.project.schematic);
        assert_eq!(after.board.tracks, before.project.board.tracks);
        let net = |reference: &str, number: &str| {
            after
                .board
                .part_by_designator(reference)
                .unwrap()
                .pads
                .as_ref()
                .unwrap()
                .0
                .iter()
                .find(|p| p.number == number)
                .unwrap()
                .net
                .clone()
                .unwrap()
        };
        let vin = net("U1", "1");
        let gnd = net("U1", "2");
        let vout = net("U1", "3");
        assert_eq!(net("C1", "1"), vin);
        assert_eq!(net("C2", "1"), vout);
        assert_eq!(net("C1", "2"), gnd);
        assert_eq!(net("C2", "2"), gnd);
        assert_ne!(vin, gnd);
        assert_ne!(vin, vout);
        assert_ne!(gnd, vout);
        assert!(vin.starts_with("sc:"));
        assert_eq!(e.preview_schematic_pcb_sync(viamind_engine::PcbSyncDirection::Forward).unwrap().changes.len(), 0);
        let bytes = file::to_string(&after);
        assert!(bytes.contains("\"version\": 6"));
        assert_eq!(file::from_str(&bytes).unwrap(), after);
        let mut log = Vec::new();
        e.with_engine(|e| e.log().write_jsonl(&mut log)).unwrap();
        assert_eq!(EventLog::read_jsonl(log.as_slice()).unwrap().replay(), after);
    }
}
#[test]
fn malformed_geometry_mapping_bounds_and_identity_preserve_entire_session() {
    for failure in 0..10 {
        let e = fixture();
        let mut req = request(&e, "U1");
        match failure {
            0 => req.placement.footprint.pads.pop().map(|_| ()).unwrap(),
            1 => req.placement.footprint.pads[1].number = "1".into(),
            2 => req.placement.footprint.pads[0].width = Nm(0),
            3 => req.placement.rotation_mdeg = 45_000,
            4 => req.placement.position.x = Nm(i64::MAX),
            5 => {
                req.placement.position.x = Nm(10_000_000_000);
                req.placement.rotation_mdeg = 0;
                req.placement.side = Side::Top;
            }
            6 => req.placement.symbol_id = ObjectId::new(),
            7 => req.placement.footprint.name = " malformed ".into(),
            8 => req.placement.footprint.pads[0].number = "OTHER".into(),
            9 => req.expected_project = ObjectId::new(),
            _ => unreachable!(),
        }
        let checkpoint = e.session_state();
        let status = e.document_status();
        let len = e.with_engine(|e| e.log().len());
        assert!(e.preview_schematic_pcb_component(req.clone()).is_err(), "failure{failure}");
        let command =
            CreateSchematicPcbComponent { expected: checkpoint.project.clone(), source_cursor: req.source_cursor, placement: req.placement };
        if failure != 9 {
            assert!(e.execute_typed(command, Origin::User).is_err(), "apply failure{failure}");
        }
        assert_eq!(e.session_state(), checkpoint);
        assert_eq!(e.document_status(), status);
        assert_eq!(e.with_engine(|e| e.log().len()), len);
    }
}
#[test]
fn duplicate_reference_link_and_assignment_mismatch_are_explicit() {
    let e = fixture();
    let req = request(&e, "U1");
    let command = e.preview_schematic_pcb_component(req).unwrap().command;
    e.execute_typed(command, Origin::User).unwrap();
    assert!(e.preview_schematic_pcb_component(request(&e, "U1")).unwrap_err().to_string().contains("already has a linked"));
    let mut req = request(&e, "C1");
    e.execute("add_part", json!({"designator":"C1","value":"independent","footprint":"other","x_mm":1,"y_mm":1}), Origin::User).unwrap();
    req.source_cursor = e.event_cursor();
    assert!(e.preview_schematic_pcb_component(req).unwrap_err().to_string().contains("reference is already"));
    let mut project = fixture().read(Clone::clone);
    let symbol = project.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.values_mut().next().unwrap();
    let reference = symbol.designator.clone();
    symbol.footprint = Some("Assigned geometry".into());
    let e = EngineHandle::new(project);
    assert!(e.preview_schematic_pcb_component(request(&e, &reference)).unwrap_err().to_string().contains("assignment differs"));
}
#[test]
fn stale_exact_project_sequence_reopen_and_failed_batch_do_not_create_parts() {
    let e = fixture();
    let old = e.preview_schematic_pcb_component(request(&e, "U1")).unwrap().command;
    e.execute("add_part", json!({"designator":"R99","value":"edit","footprint":"other","x_mm":1,"y_mm":1}), Origin::agent("other")).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(old.clone(), Origin::User).is_err());
    assert!(e.execute(CreateSchematicPcbComponent::NAME, serde_json::to_value(old).unwrap(), Origin::User).is_err());
    let mut wrong = e.preview_schematic_pcb_component(request(&e, "U1")).unwrap().command;
    wrong.expected.name = "forged".into();
    assert!(e.execute_typed(wrong, Origin::User).is_err());
    let fresh = e.preview_schematic_pcb_component(request(&e, "U1")).unwrap().command;
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: CreateSchematicPcbComponent::NAME.into(), input: serde_json::to_value(&fresh).unwrap() },
                CommandCall { name: "not_a_command".into(), input: json!({}) }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    let reopened = e.session_state();
    assert!(e.execute_typed(fresh, Origin::User).is_err());
    assert_eq!(e.session_state(), reopened);
}
#[test]
fn result_object_budget_is_checked_before_commit_and_preview() {
    let mut project = fixture().read(Clone::clone);
    let count = project.schematic.sheets.len()
        + project
            .schematic
            .sheets
            .values()
            .map(|s| s.content.as_ref().map_or(0, |c| c.object_count() + c.symbols.values().map(|s| s.pins.len()).sum::<usize>()))
            .sum::<usize>();
    for n in count..9999 {
        let part = viamind_doc::Part {
            id: ObjectId::new(),
            schematic_symbol: None,
            pads: None,
            designator: format!("Z{n}"),
            value: String::new(),
            footprint: String::new(),
            position: Point::ORIGIN,
            side: Side::Top,
            rotation_mdeg: 0,
        };
        project.board.parts.insert(part.id, part);
    }
    let e = EngineHandle::new(project);
    let req = request(&e, "U1");
    let before = e.session_state();
    assert!(e.preview_schematic_pcb_component(req.clone()).unwrap_err().to_string().contains("10000 objects"));
    let command = CreateSchematicPcbComponent { expected: before.project.clone(), source_cursor: before.cursor, placement: req.placement };
    assert!(e.execute_typed(command, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
