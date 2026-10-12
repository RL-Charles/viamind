use serde_json::json;
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, PinType, SchematicContent, SchematicSymbol, SymbolPin},
    symbol_package::{PackageUnit, SchematicPackage, UnitRole},
};
use viamind_engine::{AnnotationMode, AnnotationOptions, AnnotationOrder, CommandCall, EngineHandle, EventLog, Origin};
use viamind_geom::Nm;
fn id(n: u64) -> ObjectId {
    format!("00000000-0000-0000-0000-{n:012x}").parse().unwrap()
}
fn symbol(n: u64, reference: &str, x: i64, pin: &str) -> SchematicSymbol {
    SchematicSymbol {
        id: id(n),
        designator: reference.into(),
        value: "Shared original value".into(),
        footprint: Some("OriginalSOIC".into()),
        position: Coordinate { x: Nm(x), y: Nm(0) },
        rotation_quarters: 3,
        mirrored: true,
        width: Nm(4_000_001),
        height: Nm(6_000_003),
        pins: vec![SymbolPin {
            id: id(n + 10_000),
            number: pin.into(),
            name: "Preserved".into(),
            position: Coordinate { x: Nm(-3_000_000), y: Nm(0) },
            electrical: PinType::Passive,
        }],
    }
}
fn fixture() -> Project {
    let mut p = Project::new("Package annotation");
    let mut c = SchematicContent::default();
    for (base, x) in [(100, 10), (200, 30)] {
        let mut units = vec![];
        for (i, key) in ["P", "D", "C", "B", "A"].into_iter().enumerate() {
            let symbol = symbol(base + i as u64, if base == 100 { "U?" } else { "u?" }, if key == "A" { x } else { -100 }, &(i + 1).to_string());
            units.push(PackageUnit {
                key: key.into(),
                name: format!("Unit {key}"),
                role: if key == "P" { UnitRole::Power } else { UnitRole::Functional },
                symbol_id: symbol.id,
            });
            c.symbols.insert(symbol.id, symbol);
        }
        c.packages.insert(id(base + 500), SchematicPackage { id: id(base + 500), template_name: "Independent five-unit fixture".into(), units });
    }
    for (n, x) in [(301, 20), (302, 40)] {
        c.symbols.insert(id(n), symbol(n, if n == 301 { "U" } else { "u" }, x, "1"));
    }
    let mut s = Sheet::new("Complete packages");
    s.id = id(1);
    s.content = Some(c);
    p.schematic.sheets.insert(s.id, s);
    p
}
fn reset(start: u32) -> AnnotationOptions {
    AnnotationOptions { mode: AnnotationMode::ResetAll, start, ..Default::default() }
}
fn reference(p: &Project, n: u64) -> &str {
    &p.schematic.sheets[&id(1)].content.as_ref().unwrap().symbols[&id(n)].designator
}
fn group_reference(p: &mut Project, group: u64, reference: &str) {
    let c = p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap();
    let ids: Vec<_> = c.packages[&id(group)].units.iter().map(|u| u.symbol_id).collect();
    for id in ids {
        c.symbols.get_mut(&id).unwrap().designator = reference.into();
    }
}
#[test]
fn package_owners_use_key_anchor_and_one_number_with_atomic_native_log_undo() {
    let original = fixture();
    let e = EngineHandle::new(original.clone());
    let before = e.session_state();
    let events = e.subscribe();
    let preview = e.preview_schematic_annotation(reset(5)).unwrap();
    assert_eq!(e.session_state(), before);
    assert_eq!(preview.symbols, 12);
    assert_eq!(preview.packages, 2);
    assert_eq!(preview.changes.len(), 12);
    assert_eq!(preview.changes.iter().take(5).map(|c| c.unit_key.as_deref().unwrap()).collect::<Vec<_>>(), ["A", "B", "C", "D", "P"]);
    assert!(preview.changes.iter().take(5).all(|c| c.package_id == Some(id(600)) && c.after == "U5"));
    assert_eq!(preview.changes[5].symbol_id, id(301));
    assert_eq!(preview.changes[5].package_id, None);
    e.execute_typed(preview.command, Origin::agent("owner")).unwrap();
    let changed = e.read(Clone::clone);
    for n in 100..105 {
        assert_eq!(reference(&changed, n), "U5");
    }
    for n in 200..205 {
        assert_eq!(reference(&changed, n), "U7");
    }
    assert_eq!(reference(&changed, 301), "U6");
    assert_eq!(reference(&changed, 302), "U8");
    let mut restored_fields = changed.clone();
    for (sid, s) in &mut restored_fields.schematic.sheets {
        for (oid, symbol) in &mut s.content.as_mut().unwrap().symbols {
            symbol.designator = original.schematic.sheets[sid].content.as_ref().unwrap().symbols[oid].designator.clone();
        }
    }
    assert_eq!(restored_fields, original);
    let mut log = EventLog::new(original.clone());
    log.append(events.recv().unwrap());
    let mut bytes = vec![];
    log.write_jsonl(&mut bytes).unwrap();
    assert_eq!(EventLog::read_jsonl(std::io::Cursor::new(bytes)).unwrap().replay(), changed);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("package.viamind.json");
    viamind_doc::file::save(&changed, &path).unwrap();
    assert_eq!(viamind_doc::file::load(&path).unwrap(), changed);
    assert!(std::fs::read_to_string(path).unwrap().contains("\"version\": 11"));
    e.undo_own(Origin::agent("owner")).unwrap();
    assert_eq!(e.read(Clone::clone), original);
    e.redo_own(Origin::agent("owner")).unwrap();
    assert_eq!(e.read(Clone::clone), changed);
    let no_op = e.preview_schematic_annotation(reset(5)).unwrap();
    assert!(no_op.changes.is_empty());
    let state = e.session_state();
    e.execute_typed(no_op.command, Origin::User).unwrap();
    assert_eq!(e.session_state(), state);
}
#[test]
fn retained_package_reference_reserved_once_and_alias_conflicts_across_owners_only() {
    let mut p = fixture();
    group_reference(&mut p, 600, "u0007");
    let e = EngineHandle::new(p.clone());
    let preview = e.preview_schematic_annotation(AnnotationOptions::default()).unwrap();
    assert_eq!(preview.changes.len(), 7);
    assert!(preview.changes.iter().all(|c| c.package_id != Some(id(600))));
    e.execute("annotate_schematic", json!(preview.command), Origin::User).unwrap();
    let after = e.read(Clone::clone);
    assert_eq!(reference(&after, 100), "u0007");
    assert_eq!(reference(&after, 200), "U2");
    assert_eq!(reference(&after, 301), "U1");
    group_reference(&mut p, 700, "U7");
    let e = EngineHandle::new(p);
    let state = e.session_state();
    assert!(e.preview_schematic_annotation(AnnotationOptions::default()).unwrap_err().to_string().contains("ambiguous retained"));
    assert_eq!(e.session_state(), state);
}
#[test]
fn missing_stale_sequence_reopen_and_exact_membership_guards_refuse_unchanged() {
    let e = EngineHandle::new(fixture());
    let preview = e.preview_schematic_annotation(reset(1)).unwrap();
    let mut missing = preview.command.clone();
    missing.expected_cursor = None;
    let state = e.session_state();
    assert!(e.execute_typed(missing, Origin::User).unwrap_err().to_string().contains("requires expected_cursor"));
    assert_eq!(e.session_state(), state);
    e.execute("add_part", json!({"designator":"R1","value":"x","footprint":"x","x_mm":0,"y_mm":0}), Origin::agent("peer")).unwrap();
    let state = e.session_state();
    assert!(e.execute("annotate_schematic", json!(preview.command), Origin::User).unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), state);
    let fresh = e.preview_schematic_annotation(reset(1)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    let state = e.session_state();
    assert!(e.execute_typed(fresh.command, Origin::User).is_err());
    assert_eq!(e.session_state(), state);
    let mut malformed = fixture();
    group_reference(&mut malformed, 600, "U2");
    malformed.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap().symbols.get_mut(&id(100)).unwrap().designator = "U3".into();
    let e = EngineHandle::new(malformed);
    let state = e.session_state();
    assert!(e.preview_schematic_annotation(reset(1)).is_err());
    assert_eq!(e.session_state(), state);
}
#[test]
fn both_commands_in_batch_use_initial_cursor_and_one_undo() {
    let p = fixture();
    let e = EngineHandle::new(p.clone());
    let first = e.preview_schematic_annotation(reset(1)).unwrap().command;
    let offline = EngineHandle::new(p.clone());
    offline.execute_typed(offline.preview_schematic_annotation(reset(1)).unwrap().command, Origin::User).unwrap();
    let mut second = offline.preview_schematic_annotation(reset(10)).unwrap().command;
    second.expected_cursor = first.expected_cursor;
    e.execute_batch(
        vec![
            CommandCall { name: "annotate_schematic".into(), input: json!(first) },
            CommandCall { name: "annotate_schematic".into(), input: json!(second) },
        ],
        Origin::User,
        None,
    )
    .unwrap();
    assert_eq!(reference(&e.read(Clone::clone), 100), "U10");
    e.undo_own(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), p);
}
#[test]
fn malformed_excess_known_raw_rows_refuse_before_poison_and_leave_state_unchanged() {
    let e = EngineHandle::new(fixture());
    let mut input = json!(e.preview_schematic_annotation(reset(1)).unwrap().command);
    let sheet = input["expected_sheets"].as_object_mut().unwrap().values_mut().next().unwrap();
    let symbols = sheet["content"]["symbols"].as_object_mut().unwrap();
    for n in 0..2001 {
        symbols.insert(format!("poison{n}"), serde_json::Value::Null);
    }
    let state = e.session_state();
    let error = e.execute("annotate_schematic", input, Origin::User).unwrap_err().to_string();
    assert!(error.contains("2000 symbols"), "{error}");
    assert_eq!(e.session_state(), state);
}
#[test]
fn legacy_ungrouped_omission_preserves_native_bytes_and_absent_group_row_fields() {
    let mut p = fixture();
    p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap().packages.clear();
    for (n, s) in p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap().symbols.values_mut().enumerate() {
        s.designator = format!("U{}", n + 1);
    }
    let before = viamind_doc::file::to_string(&p);
    let e = EngineHandle::new(p.clone());
    let preview = e.preview_schematic_annotation(AnnotationOptions::default()).unwrap();
    assert_eq!(preview.packages, 0);
    assert!(preview.changes.is_empty());
    let mut command = preview.command;
    command.expected_cursor = None;
    assert!(json!(&command).get("expected_cursor").is_none());
    e.execute_typed(command, Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), p);
    assert_eq!(viamind_doc::file::to_string(&e.read(Clone::clone)), before);
}
#[test]
fn axis_choice_owner_uuid_ties_explicit_sheet_order_and_exhaustion_are_deterministic() {
    let mut p = fixture();
    let c = p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap();
    c.symbols.get_mut(&id(104)).unwrap().position = Coordinate { x: Nm(20), y: Nm(30) };
    c.symbols.get_mut(&id(204)).unwrap().position = Coordinate { x: Nm(20), y: Nm(30) };
    c.symbols.get_mut(&id(301)).unwrap().position = Coordinate { x: Nm(20), y: Nm(30) };
    let e = EngineHandle::new(p);
    let preview = e.preview_schematic_annotation(AnnotationOptions { order: AnnotationOrder::YThenX, ..reset(1) }).unwrap();
    assert_eq!(preview.changes[0].symbol_id, id(302));
    assert_eq!(preview.changes[1].symbol_id, id(301));
    assert_eq!(preview.changes[2].package_id, Some(id(600)));
    let state = e.session_state();
    assert!(e.preview_schematic_annotation(reset(999_999_999)).is_err());
    assert!(e.preview_schematic_annotation(AnnotationOptions { sheet_order: vec![id(2)], ..reset(1) }).is_err());
    assert_eq!(e.session_state(), state);
}
#[test]
fn annotation_ignores_huge_unused_board_assembly_and_rejects_malformed_package_ownership() {
    let mut p = fixture();
    let part = viamind_doc::Part {
        holes: None,
        artwork: None,
        id: id(900),
        assembly: Some(Box::new(viamind_doc::assembly::AssemblyMetadata { manufacturer: Some("x".repeat(3 * 1024 * 1024)), ..Default::default() })),
        schematic_symbol: None,
        schematic_package: None,
        pads: None,
        designator: "BOARD1".into(),
        value: "unchanged".into(),
        footprint: "explicit".into(),
        position: viamind_geom::Point::new(Nm(1), Nm(3)),
        rotation_mdeg: 0,
        side: viamind_doc::Side::Top,
    };
    p.board.parts.insert(part.id, part);
    let e = EngineHandle::new(p.clone());
    let preview = e.preview_schematic_annotation(reset(1)).unwrap();
    e.execute_typed(preview.command, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.clone()), p.board);
    e.undo_own(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), p);
    for variant in 0..5 {
        let mut p = fixture();
        let c = p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap();
        match variant {
            0 => c.packages.get_mut(&id(600)).unwrap().id = id(999),
            1 => c.packages.get_mut(&id(600)).unwrap().units[0].symbol_id = id(999),
            2 => c.packages.get_mut(&id(600)).unwrap().units[0].key = "A".into(),
            3 => c.packages.get_mut(&id(600)).unwrap().template_name = "x".repeat(256 * 1024 + 1),
            _ => p.id = id(600),
        }
        let e = EngineHandle::new(p);
        let before = e.session_state();
        assert!(e.preview_schematic_annotation(reset(1)).is_err(), "variant{variant}");
        assert_eq!(e.session_state(), before);
    }
}
#[test]
fn complete_explicit_sheet_order_changes_owner_order_without_splitting_packages() {
    let mut p = fixture();
    let c = p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap();
    let group = c.packages.remove(&id(700)).unwrap();
    let mut second = SchematicContent::default();
    for unit in &group.units {
        let symbol = c.symbols.remove(&unit.symbol_id).unwrap();
        second.symbols.insert(symbol.id, symbol);
    }
    second.packages.insert(group.id, group);
    let mut sheet = Sheet::new("Second complete package");
    sheet.id = id(2);
    sheet.content = Some(second);
    p.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(p);
    let preview = e.preview_schematic_annotation(AnnotationOptions { sheet_order: vec![id(2), id(1)], ..reset(5) }).unwrap();
    assert_eq!(preview.sheet_order, vec![id(2), id(1)]);
    assert!(preview.changes[..5].iter().all(|c| c.package_id == Some(id(700)) && c.after == "U5"));
    assert!(preview.changes[5..10].iter().all(|c| c.package_id == Some(id(600)) && c.after == "U6"));
    assert_eq!(preview.changes[10].after, "U7");
    assert_eq!(preview.changes[11].after, "U8");
}
