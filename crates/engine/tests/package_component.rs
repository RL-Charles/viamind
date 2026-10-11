use serde_json::json;
use std::collections::BTreeSet;
use viamind_doc::{
    ObjectId, Project, Sheet, Side,
    footprint::{Footprint, FootprintPad, LocalPosition},
    schematic::{Coordinate, SchematicContent},
};
use viamind_engine::{
    Command, CommandCall, CreatePackagePcbComponent, EngineHandle, EventLog, LinkSchematicPackage, Origin, PackageComponentPlacement,
    PackageComponentRequest, PlaceSymbolPackage, UnitPose,
};
use viamind_geom::{Nm, Point};

fn fixture() -> EngineHandle {
    let mut project = Project::new("Complete physical package");
    let mut sheet = Sheet::new("Authored units");
    sheet.content = Some(SchematicContent::default());
    project.schematic.sheets.insert(sheet.id, sheet.clone());
    let engine = EngineHandle::new(project);
    let template =
        viamind_doc::symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json"))
            .unwrap();
    let placements = template
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| UnitPose {
            key: u.key.clone(),
            position: Coordinate { x: Nm(20_000_000 * i as i64 + 1), y: Nm(3) },
            rotation_quarters: (i % 4) as u8,
            mirrored: i % 2 == 1,
        })
        .collect();
    engine
        .execute_typed(
            PlaceSymbolPackage {
                expected_project: engine.read(|p| p.id),
                source_cursor: engine.event_cursor(),
                expected: sheet,
                template,
                designator: "U1".into(),
                placements,
            },
            Origin::User,
        )
        .unwrap();
    engine
}
fn request(engine: &EngineHandle) -> PackageComponentRequest {
    PackageComponentRequest {
        expected_project: engine.read(|p| p.id),
        source_cursor: engine.event_cursor(),
        placement: PackageComponentPlacement {
            package_id: engine.read(|p| *p.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().packages.keys().next().unwrap()),
            footprint: Footprint {
                name: "Authored-14pad".into(),
                pads: (1..=14)
                    .rev()
                    .map(|i| FootprintPad {
                        number: i.to_string(),
                        position: LocalPosition { x: Nm(2_000_000 * i), y: Nm(257) },
                        width: Nm(800_001),
                        height: Nm(600_003),
                    })
                    .collect(),
            },
            position: Point::new(Nm(40_000_001), Nm(30_000_003)),
            side: Side::Bottom,
            rotation_mdeg: 90_000,
        },
    }
}
#[test]
fn complete_five_unit_fourteen_pin_union_creates_exactly_one_physical_part_with_independent_terminal_oracle() {
    for typed in [true, false] {
        let e = fixture();
        let before = e.session_state();
        let input = request(&e);
        let preview = e.preview_package_pcb_component(input.clone()).unwrap();
        assert_eq!(e.session_state(), before);
        assert_eq!(preview.unit_count, 5);
        assert_eq!(preview.pins.len(), 14);
        assert_eq!(serde_json::to_value(&preview).unwrap(), serde_json::to_value(e.preview_package_pcb_component(input.clone()).unwrap()).unwrap());
        let numbers: BTreeSet<_> = preview.pins.iter().map(|p| p.number.as_str()).collect();
        assert_eq!(numbers, (1..=14).map(|i| i.to_string()).collect::<Vec<_>>().iter().map(String::as_str).collect());
        assert_eq!(
            preview.pins.iter().map(|p| p.net.as_str()).collect::<BTreeSet<_>>().len(),
            14,
            "roles and separate power unit never connect isolated pins"
        );
        for pin in &preview.pins {
            let n = pin.number.parse::<usize>().unwrap();
            let unit = if n <= 12 { ["A", "B", "C", "D"][(n - 1) / 3] } else { "P" };
            assert_eq!(pin.unit_key, unit);
            e.read(|p| {
                let content = p.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
                assert!(content.symbols[&pin.symbol_id].pins.iter().any(|x| x.id == pin.pin_id && x.number == pin.number));
            });
        }
        let result = if typed {
            e.execute_typed(preview.command, Origin::agent("creator"))
        } else {
            e.execute(CreatePackagePcbComponent::NAME, serde_json::to_value(preview.command).unwrap(), Origin::agent("creator"))
        }
        .unwrap();
        assert_eq!(result.created.len(), 1);
        assert_eq!(result.deltas.len(), 1);
        let after = e.read(Clone::clone);
        let part = &after.board.parts[&result.created[0]];
        assert_eq!(after.board.parts.len(), 1);
        assert_eq!(part.schematic_package, Some(input.placement.package_id));
        assert_eq!(part.schematic_symbol, None);
        assert_eq!(after.schematic, before.project.schematic);
        assert_eq!((part.designator.as_str(), part.value.as_str()), ("U1", "Authored fourteen pins"));
        for pad in &part.pads.as_ref().unwrap().0 {
            let n = pad.number.parse::<i64>().unwrap();
            // Independent bottom X reflection then 90-degree y-down rotation: (x,y)->(y,x).
            assert_eq!(part.local_to_world_nm_exact(pad.position).unwrap(), Point::new(Nm(40_000_258), Nm(30_000_003 + 2_000_000 * n)));
            assert_eq!(pad.net.as_ref().unwrap(), &preview.pins.iter().find(|p| p.number == pad.number).unwrap().net);
        }
        assert_eq!(viamind_doc::file::minimum_version(&after), 12);
        assert_eq!(viamind_doc::file::from_str(&viamind_doc::file::to_string(&after)).unwrap(), after);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
        let mut bytes = vec![];
        e.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
        assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), after);
        let mut text = String::from_utf8(bytes).unwrap();
        let end = text.find('\n').unwrap();
        let mut header: serde_json::Value = serde_json::from_str(&text[..end]).unwrap();
        header["version"] = json!(11);
        text = serde_json::to_string(&header).unwrap() + &text[end..];
        assert!(EventLog::read_jsonl(text.as_bytes()).is_err(), "after package-link event requires12 even with base11");
    }
}
#[test]
fn incomplete_extra_mismatched_duplicate_and_stale_proposals_refuse_without_session_changes() {
    let e = fixture();
    let valid = request(&e);
    let before = e.session_state();
    for kind in 0..7 {
        let mut bad = valid.clone();
        match kind {
            0 => {
                bad.placement.footprint.pads.pop();
            }
            1 => {
                let mut p = bad.placement.footprint.pads[0].clone();
                p.number = "15".into();
                bad.placement.footprint.pads.push(p);
            }
            2 => bad.placement.footprint.pads[0].number = "1".into(),
            3 => bad.placement.footprint.name = "different".into(),
            4 => bad.placement.package_id = ObjectId::new(),
            5 => bad.placement.rotation_mdeg = 123,
            _ => bad.placement.position.x = Nm(10_000_000_000),
        }
        assert!(e.preview_package_pcb_component(bad).is_err(), "negative kind{kind}");
        assert_eq!(e.session_state(), before);
    }
    let preview = e.preview_package_pcb_component(valid.clone()).unwrap();
    e.execute("add_part", json!({"designator":"PEER1","value":"keep","footprint":"F","x_mm":1,"y_mm":2}), Origin::agent("peer")).unwrap();
    e.undo(Origin::agent("peer")).unwrap();
    let peer = e.session_state();
    assert_eq!(peer.project, before.project);
    assert_ne!(peer.cursor, before.cursor);
    assert!(e.execute_typed(preview.command, Origin::User).is_err());
    assert_eq!(e.session_state(), peer, "equal design after undo cannot bypass fullcursor");
    let preview = e.preview_package_pcb_component(request(&e)).unwrap();
    assert!(
        e.execute_batch(
            vec![
                CommandCall::new(CreatePackagePcbComponent::NAME, serde_json::to_value(preview.command).unwrap()),
                CommandCall::new("delete_part", json!({"id":ObjectId::new()}))
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), peer, "later batch failure rolls back all new physical identities");
}
#[test]
fn explicit_package_unlink_relink_preserves_geometry_nets_and_requires_original_full_guard() {
    let e = fixture();
    let input = request(&e);
    let preview = e.preview_package_pcb_component(input.clone()).unwrap();
    let result = e.execute_typed(preview.command, Origin::User).unwrap();
    let id = result.created[0];
    let before = e.session_state();
    let unlink = LinkSchematicPackage { expected: before.project.clone(), source_cursor: before.cursor, part_id: id, package_id: None };
    e.execute_typed(unlink.clone(), Origin::User).unwrap();
    let unlinked = e.session_state();
    let mut expected = before.project.clone();
    expected.board.parts.get_mut(&id).unwrap().schematic_package = None;
    assert_eq!(unlinked.project, expected);
    assert!(e.execute_typed(unlink, Origin::User).is_err());
    assert_eq!(e.session_state(), unlinked);
    e.execute_typed(
        LinkSchematicPackage {
            expected: unlinked.project,
            source_cursor: unlinked.cursor,
            part_id: id,
            package_id: Some(input.placement.package_id),
        },
        Origin::User,
    )
    .unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    assert!(e.preview_package_pcb_component(request(&e)).is_err(), "second physical owner rejected");
}
