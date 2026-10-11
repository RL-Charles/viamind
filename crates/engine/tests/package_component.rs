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

#[test]
fn every_unit_and_package_query_resolves_one_owner_and_all_actual_transformed_pin_pad_pairs() {
    use viamind_engine::{ComponentLinkQuery, ComponentLinkStatus, PinPadCoverage, PinPadLinkQuery};
    let e = fixture();
    let input = request(&e);
    let command = e.preview_package_pcb_component(input.clone()).unwrap().command;
    let id = e.execute_typed(command, Origin::User).unwrap().created[0];
    let before = e.session_state();
    let all = e.query_linked_components(&ComponentLinkQuery::default()).unwrap();
    assert_eq!(all.total, 1, "five units share one physical row");
    let row = &all.components[0];
    assert_eq!(row.status, ComponentLinkStatus::Linked);
    assert_eq!(row.symbol_id, None, "package identity never substitutes a representative unit");
    assert_eq!(row.package_id, Some(input.placement.package_id));
    assert_eq!(row.package_units.iter().map(|u| u.key.as_str()).collect::<Vec<_>>(), ["A", "B", "C", "D", "P"]);
    for component_id in [id, input.placement.package_id].into_iter().chain(row.package_units.iter().map(|u| u.symbol_id)) {
        let result = e.query_linked_components(&ComponentLinkQuery { object_ids: vec![component_id], ..Default::default() }).unwrap();
        assert!(result.unknown_ids.is_empty());
        assert_eq!(serde_json::to_value(&result.components).unwrap(), serde_json::to_value(&all.components).unwrap());
        let terminals = e.query_linked_pin_pads(&PinPadLinkQuery { component_id, offset: 0, limit: Some(100) }).unwrap();
        assert_eq!(terminals.coverage, PinPadCoverage::Linked);
        assert_eq!(terminals.total, 14);
        for terminal in &terminals.terminals {
            assert_eq!(terminal.package_id, row.package_id);
            assert!(terminal.navigable, "{:?}", terminal.unsupported_reasons);
            let n = terminal.number.parse::<i64>().unwrap();
            assert_eq!(terminal.board_position, Some(Point::new(Nm(40_000_258), Nm(30_000_003 + 2_000_000 * n))));
            e.read(|p| {
                let content = p.schematic.sheets[&terminal.sheet_id.unwrap()].content.as_ref().unwrap();
                let symbol = &content.symbols[&terminal.symbol_id.unwrap()];
                let pin = symbol.pins.iter().find(|p| Some(p.id) == terminal.pin_id).unwrap();
                let transformed = symbol.pin_position(pin.position).unwrap();
                assert_eq!(terminal.schematic_position, Some(Point::new(transformed.x, transformed.y)));
                assert_eq!(
                    terminal.unit_key.as_ref().unwrap(),
                    &content.packages[&input.placement.package_id].units.iter().find(|u| u.symbol_id == symbol.id).unwrap().key
                );
            });
        }
    }
    assert_eq!(e.session_state(), before);
}

#[test]
fn complete_package_forward_and_back_are_atomic_preserve_geometry_metadata_and_member_identity() {
    use viamind_engine::PcbSyncDirection;
    let creator = fixture();
    let command = creator.preview_package_pcb_component(request(&creator)).unwrap().command;
    let id = creator.execute_typed(command, Origin::User).unwrap().created[0];
    let mut project = creator.read(Clone::clone);
    let part = project.board.parts.get_mut(&id).unwrap();
    part.designator = "PCB9".into();
    part.value = "Physical value".into();
    part.assembly = Some(Box::new(viamind_doc::AssemblyMetadata { manufacturer: Some("Explicit user provenance".into()), ..Default::default() }));
    for pad in &mut part.pads.as_mut().unwrap().0 {
        pad.net = None;
    }
    for sheet in project.schematic.sheets.values_mut() {
        for symbol in sheet.content.iter_mut().flat_map(|c| c.symbols.values_mut()) {
            symbol.footprint = None;
        }
    }
    let e = EngineHandle::new(project.clone());
    let before = e.session_state();
    let forward = e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap();
    assert!(forward.unlinked_symbols.is_empty());
    assert!(forward.unlinked_parts.is_empty());
    assert_eq!(forward.changes.iter().filter(|c| c.field == "pad_net").count(), 14);
    assert!(forward.changes.iter().all(|c| c.package_id == project.board.parts[&id].schematic_package && c.unit_key.is_some()));
    assert_eq!(e.session_state(), before, "preview immutable");
    e.execute_typed(forward.command, Origin::agent("forward")).unwrap();
    let after = e.read(Clone::clone);
    let mut normalized = after.clone();
    normalized.board.parts.get_mut(&id).unwrap().designator = "PCB9".into();
    normalized.board.parts.get_mut(&id).unwrap().value = "Physical value".into();
    for pad in &mut normalized.board.parts.get_mut(&id).unwrap().pads.as_mut().unwrap().0 {
        pad.net = None;
    }
    assert_eq!(normalized, project, "only declared forward fields change");
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), project);
    let back = e.preview_schematic_pcb_sync(PcbSyncDirection::Back).unwrap();
    assert_eq!(back.changes.len(), 15, "three shared fields in every one of five units");
    assert_eq!(back.changes.iter().filter_map(|c| c.unit_key.as_deref()).collect::<BTreeSet<_>>(), BTreeSet::from(["A", "B", "C", "D", "P"]));
    e.execute_typed(back.command, Origin::User).unwrap();
    let back_after = e.read(Clone::clone);
    assert_eq!(back_after.board, project.board);
    let original_content = project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
    let actual_content = back_after.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
    assert_eq!(actual_content.packages, original_content.packages);
    for symbol in actual_content.symbols.values() {
        let mut normalized = symbol.clone();
        assert_eq!(
            (symbol.designator.as_str(), symbol.value.as_str(), symbol.footprint.as_deref()),
            ("PCB9", "Physical value", Some("Authored-14pad"))
        );
        normalized.designator = "U1".into();
        normalized.value = "Authored fourteen pins".into();
        normalized.footprint = None;
        assert_eq!(normalized, original_content.symbols[&symbol.id]);
    }
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), back_after);
}
