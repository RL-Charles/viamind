use serde_json::json;
use viamind_doc::{file, schematic::SchematicSymbol};
use viamind_engine::{Command, EngineHandle, EventLog, LinkSchematicSymbol, Origin, SchematicLinkTarget};
fn fixture() -> (EngineHandle, SchematicSymbol) {
    let project = file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
    let symbol = project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().symbols.values().next().unwrap().clone();
    let e = EngineHandle::new(project);
    e.execute("add_part", json!({"designator":"PCB1","value":"old","footprint":"original","x_mm":4,"y_mm":8}), Origin::User).unwrap();
    (e, symbol)
}
fn link(e: &EngineHandle, symbol: &SchematicSymbol) -> LinkSchematicSymbol {
    LinkSchematicSymbol {
        expected_project: e.read(|p| p.id),
        expected_part: e.read(|p| p.board.parts.values().next().unwrap().clone()),
        target: SchematicLinkTarget::Symbol { expected: symbol.clone() },
    }
}
#[test]
fn link_is_guarded_shared_undoable_and_versioned_even_if_final_state_unlinked() {
    for typed in [true, false] {
        let (e, symbol) = fixture();
        let before = e.session_state();
        let command = link(&e, &symbol);
        if typed {
            e.execute_typed(command.clone(), Origin::User)
        } else {
            e.execute(LinkSchematicSymbol::NAME, serde_json::to_value(&command).unwrap(), Origin::agent("sync"))
        }
        .unwrap();
        let after = e.read(Clone::clone);
        let mut normalized = after.clone();
        normalized.board.parts.values_mut().next().unwrap().schematic_symbol = None;
        assert_eq!(normalized, before.project, "linking changes only its identity association");
        assert_eq!(after.board.parts.values().next().unwrap().schematic_symbol, Some(symbol.id));
        assert_eq!(file::from_str(&file::to_string(&after)).unwrap(), after);
        let checkpoint = e.session_state();
        assert!(e.execute_typed(command, Origin::User).is_err());
        assert_eq!(e.session_state(), checkpoint, "old part guard cannot apply after linking");
        e.execute_typed(link(&e, &symbol), Origin::User).unwrap();
        assert_eq!(e.session_state(), checkpoint, "repeated exact link is a true no-op");
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
        e.execute_typed(LinkSchematicSymbol { target: SchematicLinkTarget::Unlink, ..link(&e, &symbol) }, Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        let mut bytes = Vec::new();
        e.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
        assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), before.project);
        let downgraded = String::from_utf8(bytes).unwrap().replace("\"version\":6", "\"version\":5");
        assert!(EventLog::read_jsonl(downgraded.as_bytes()).is_err(), "historic linked snapshots require version6 even after unlink");
    }
}
#[test]
fn stale_symbol_duplicate_link_and_missing_explicit_target_preserve_session() {
    let (e, symbol) = fixture();
    let mut stale = link(&e, &symbol);
    if let SchematicLinkTarget::Symbol { expected } = &mut stale.target {
        expected.value = "stale".into();
    }
    let checkpoint = e.session_state();
    assert!(e.execute_typed(stale, Origin::User).is_err());
    assert_eq!(e.session_state(), checkpoint);
    let mut omitted = serde_json::to_value(link(&e, &symbol)).unwrap();
    omitted.as_object_mut().unwrap().remove("target");
    assert!(e.execute(LinkSchematicSymbol::NAME, omitted, Origin::User).is_err());
    assert_eq!(e.session_state(), checkpoint);
    e.execute_typed(link(&e, &symbol), Origin::User).unwrap();
    let id =
        e.execute("add_part", json!({"designator":"PCB2","value":"old","footprint":"original","x_mm":9,"y_mm":8}), Origin::User).unwrap().created[0];
    let duplicate = LinkSchematicSymbol {
        expected_project: e.read(|p| p.id),
        expected_part: e.read(|p| p.board.parts[&id].clone()),
        target: SchematicLinkTarget::Symbol { expected: symbol },
    };
    let checkpoint = e.session_state();
    assert!(e.execute_typed(duplicate, Origin::User).unwrap_err().to_string().contains("already linked"));
    assert_eq!(e.session_state(), checkpoint);
}

use viamind_doc::{ObjectId, Pad, Pads, Part, Project, Side};
use viamind_engine::{CommandCall, PcbSyncDirection, SynchronizeSchematicPcb};
use viamind_geom::{Nm, Point};
fn linked_project() -> Project {
    let mut project = file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
    let symbols: Vec<_> = project.schematic.sheets.values().flat_map(|s| s.content.iter().flat_map(|c| c.symbols.values())).cloned().collect();
    for (i, symbol) in symbols.iter().enumerate() {
        let pads = symbol
            .pins
            .iter()
            .enumerate()
            .map(|(j, pin)| Pad {
                id: ObjectId::new(),
                number: pin.number.clone(),
                position: Point::new(Nm(j as i64 * 2_000_000), Nm(0)),
                width: Nm(1_000_000),
                height: Nm(1_500_000),
                net: None,
            })
            .collect();
        let part = Part {
            id: ObjectId::new(),
            schematic_symbol: Some(symbol.id),
            pads: Some(Pads::try_from(pads).unwrap()),
            designator: format!("PCB{}", i + 1),
            value: "old board value".into(),
            footprint: "explicit-native".into(),
            position: Point::new(Nm(i as i64 * 10_000_000), Nm(7_000_000)),
            rotation_mdeg: 90_000,
            side: Side::Bottom,
        };
        project.board.parts.insert(part.id, part);
    }
    project
}
fn symbol<'a>(project: &'a Project, part: &Part) -> &'a SchematicSymbol {
    project.schematic.sheets.values().find_map(|s| s.content.as_ref().and_then(|c| c.symbols.get(&part.schematic_symbol.unwrap()))).unwrap()
}
#[test]
fn forward_preview_preserves_geometry_and_assigns_independently_expected_regulator_nets_atomically() {
    for typed in [true, false] {
        let before = linked_project();
        let e = EngineHandle::new(before.clone());
        let checkpoint = e.session_state();
        let preview = e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap();
        assert_eq!(e.session_state(), checkpoint, "preview is immutable");
        assert_eq!(
            serde_json::to_value(&preview).unwrap(),
            serde_json::to_value(e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap()).unwrap()
        );
        assert_eq!(preview.nets.len(), 3);
        assert!(preview.unlinked_parts.is_empty() && preview.unlinked_symbols.is_empty());
        if typed {
            e.execute_typed(preview.command, Origin::User)
        } else {
            e.execute(SynchronizeSchematicPcb::NAME, serde_json::to_value(preview.command).unwrap(), Origin::agent("sync"))
        }
        .unwrap();
        let after = e.read(Clone::clone);
        assert_eq!(e.event_cursor().sequence, checkpoint.cursor.sequence + 1);
        let net = |reference: &str, pin: &str| {
            after
                .board
                .parts
                .values()
                .find(|p| p.designator == reference)
                .unwrap()
                .pads
                .as_ref()
                .unwrap()
                .0
                .iter()
                .find(|p| p.number == pin)
                .unwrap()
                .net
                .clone()
                .unwrap()
        };
        assert_eq!(net("U1", "1"), net("C1", "1"));
        assert_eq!(net("U1", "2"), net("C1", "2"));
        assert_eq!(net("U1", "2"), net("C2", "2"));
        assert_eq!(net("U1", "3"), net("C2", "1"));
        assert_ne!(net("U1", "1"), net("U1", "2"));
        assert_ne!(net("U1", "2"), net("U1", "3"));
        assert_ne!(net("U1", "1"), net("U1", "3"));
        let mut normalized = after.clone();
        for part in normalized.board.parts.values_mut() {
            let old = &before.board.parts[&part.id];
            assert_eq!(part.designator, symbol(&before, old).designator);
            assert_eq!(part.value, symbol(&before, old).value);
            part.designator = old.designator.clone();
            part.value = old.value.clone();
            for pad in &mut part.pads.as_mut().unwrap().0 {
                pad.net = None;
            }
        }
        assert_eq!(normalized, before, "only references, values and pad nets change");
        assert_eq!(file::from_str(&file::to_string(&after)).unwrap(), after);
        let checkpoint = e.session_state();
        let no_op = e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap();
        assert!(no_op.changes.is_empty());
        e.execute_typed(no_op.command, Origin::User).unwrap();
        assert_eq!(e.session_state(), checkpoint);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
        let mut bytes = Vec::new();
        e.with_engine(|e| e.log().write_jsonl(&mut bytes)).unwrap();
        assert_eq!(EventLog::read_jsonl(bytes.as_slice()).unwrap().replay(), after);
    }
}
#[test]
fn back_annotation_changes_only_linked_symbol_properties_and_links_survive_renumbering() {
    let before = linked_project();
    let e = EngineHandle::new(before.clone());
    let preview = e.preview_schematic_pcb_sync(PcbSyncDirection::Back).unwrap();
    e.execute_typed(preview.command, Origin::User).unwrap();
    let after = e.read(Clone::clone);
    let mut normalized = after.clone();
    for part in before.board.parts.values() {
        let old = symbol(&before, part);
        let updated = symbol(&after, part);
        assert_eq!(
            (&updated.designator, &updated.value, updated.footprint.as_deref()),
            (&part.designator, &part.value, Some(part.footprint.as_str()))
        );
        let target = normalized.schematic.sheets.values_mut().find_map(|s| s.content.as_mut().and_then(|c| c.symbols.get_mut(&old.id))).unwrap();
        target.designator = old.designator.clone();
        target.value = old.value.clone();
        target.footprint = old.footprint.clone();
    }
    assert_eq!(normalized, before, "pins, wires, labels, positions, copper and links survive");
    let annotation = e
        .preview_schematic_annotation(viamind_engine::AnnotationOptions {
            mode: viamind_engine::AnnotationMode::ResetAll,
            start: 10,
            ..Default::default()
        })
        .unwrap();
    e.execute_typed(annotation.command, Origin::User).unwrap();
    for p in e.read(|p| p.board.parts.values().cloned().collect::<Vec<_>>()) {
        assert!(symbol(&e.read(Clone::clone), &p).id == p.schematic_symbol.unwrap());
    }
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn synchronization_rejects_stale_project_stream_and_batch_without_partial_publication() {
    let e = EngineHandle::new(linked_project());
    let old = e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap().command;
    let id = e.read(|p| *p.board.parts.keys().next().unwrap());
    e.execute("update_part", json!({"id":id,"value":"concurrent"}), Origin::agent("other")).unwrap();
    let checkpoint = e.session_state();
    let log = e.with_engine(|e| e.log().len());
    assert!(e.execute_typed(old.clone(), Origin::User).is_err());
    assert!(e.execute(SynchronizeSchematicPcb::NAME, serde_json::to_value(old).unwrap(), Origin::User).is_err());
    let fresh = e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap().command;
    let wrong_stream = SynchronizeSchematicPcb { source_cursor: EngineHandle::new(checkpoint.project.clone()).event_cursor(), ..fresh.clone() };
    assert!(e.execute_typed(wrong_stream, Origin::User).is_err());
    let mut wrong_project = fresh.clone();
    wrong_project.expected.name = "different".into();
    assert!(e.execute_typed(wrong_project, Origin::User).is_err());
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: "update_part".into(), input: json!({"id":id,"value":"batch edit"}) },
                CommandCall { name: SynchronizeSchematicPcb::NAME.into(), input: serde_json::to_value(fresh).unwrap() },
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), checkpoint);
    assert_eq!(e.with_engine(|e| e.log().len()), log);
}
#[test]
fn ambiguous_mappings_footprints_missing_links_budgets_and_reference_collisions_reject() {
    let original = linked_project();
    for alter in [
        |p: &mut Project| p.board.parts.values_mut().next().unwrap().pads = None,
        |p: &mut Project| p.board.parts.values_mut().next().unwrap().pads.as_mut().unwrap().0[0].number = "missing".into(),
        |p: &mut Project| {
            p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.values_mut().next().unwrap().footprint =
                Some("other geometry".into())
        },
        |p: &mut Project| p.board.parts.values_mut().next().unwrap().schematic_symbol = Some(ObjectId::new()),
        |p: &mut Project| p.name = "x".repeat(4097),
        |p: &mut Project| {
            let mut extra = p.board.parts.values().next().unwrap().clone();
            extra.id = ObjectId::new();
            extra.schematic_symbol = None;
            extra.pads = None;
            extra.designator = "U1".into();
            p.board.parts.insert(extra.id, extra);
        },
    ] {
        let mut project = original.clone();
        alter(&mut project);
        let e = EngineHandle::new(project);
        let checkpoint = e.session_state();
        assert!(e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).is_err());
        assert_eq!(e.session_state(), checkpoint);
    }
}
#[test]
fn routed_net_reassignment_rejects_and_existing_unchanged_routes_are_preserved() {
    let e = EngineHandle::new(linked_project());
    e.execute_typed(e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap().command, Origin::User).unwrap();
    let net = e.read(|p| p.board.parts.values().next().unwrap().pads.as_ref().unwrap().0[0].net.clone().unwrap());
    e.execute("add_track", json!({"start_x_mm":0,"start_y_mm":0,"end_x_mm":2,"end_y_mm":2,"width_mm":0.2,"layer":"top","net":net}), Origin::User)
        .unwrap();
    let checkpoint = e.session_state();
    e.execute_typed(e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap().command, Origin::User).unwrap();
    assert_eq!(e.session_state(), checkpoint, "unchanged routed assignments are a no-op");
    let mut project = checkpoint.project;
    project.board.parts.values_mut().next().unwrap().pads.as_mut().unwrap().0[0].net = Some("old assignment".into());
    let e = EngineHandle::new(project);
    let checkpoint = e.session_state();
    assert!(e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap_err().to_string().contains("routed copper"));
    assert_eq!(e.session_state(), checkpoint);
}
#[test]
fn identical_local_labels_on_two_sheets_remain_separate_while_power_labels_join() {
    let mut project = linked_project();
    let old_sheet = project.schematic.sheets.values().next().unwrap();
    let mut sheet = old_sheet.clone();
    sheet.id = ObjectId::new();
    sheet.name = "Second regulator".into();
    let content = sheet.content.as_mut().unwrap();
    let mut symbol_ids = std::collections::BTreeMap::new();
    content.symbols = std::mem::take(&mut content.symbols)
        .into_values()
        .map(|mut s| {
            let old = s.id;
            s.id = ObjectId::new();
            symbol_ids.insert(old, s.id);
            s.designator.push('0');
            for pin in &mut s.pins {
                pin.id = ObjectId::new();
            }
            (s.id, s)
        })
        .collect();
    content.wires = std::mem::take(&mut content.wires)
        .into_values()
        .map(|mut w| {
            w.id = ObjectId::new();
            (w.id, w)
        })
        .collect();
    content.labels = std::mem::take(&mut content.labels)
        .into_values()
        .map(|mut l| {
            l.id = ObjectId::new();
            (l.id, l)
        })
        .collect();
    content.junctions = std::mem::take(&mut content.junctions)
        .into_values()
        .map(|mut j| {
            j.id = ObjectId::new();
            (j.id, j)
        })
        .collect();
    let extras: Vec<_> = project.board.parts.values().cloned().collect();
    for mut part in extras {
        part.id = ObjectId::new();
        part.designator.push('0');
        part.schematic_symbol = Some(symbol_ids[&part.schematic_symbol.unwrap()]);
        for pad in &mut part.pads.as_mut().unwrap().0 {
            pad.id = ObjectId::new();
        }
        project.board.parts.insert(part.id, part);
    }
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let preview = e.preview_schematic_pcb_sync(PcbSyncDirection::Forward).unwrap();
    assert_eq!(preview.nets.len(), 5, "two VIN, two 3V3 and one project-wide GND");
    e.execute_typed(preview.command, Origin::User).unwrap();
    e.read(|p| {
        let net = |reference: &str, pin: &str| {
            p.board
                .parts
                .values()
                .find(|part| part.designator == reference)
                .unwrap()
                .pads
                .as_ref()
                .unwrap()
                .0
                .iter()
                .find(|pad| pad.number == pin)
                .unwrap()
                .net
                .as_deref()
                .unwrap()
        };
        assert_ne!(net("U1", "1"), net("U10", "1"));
        assert_ne!(net("U1", "3"), net("U10", "3"));
        assert_eq!(net("U1", "2"), net("U10", "2"));
        assert_eq!(net("C10", "1"), net("U10", "1"));
        assert_eq!(net("C20", "1"), net("U10", "3"));
    });
}
