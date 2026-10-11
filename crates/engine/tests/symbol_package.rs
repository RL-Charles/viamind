use serde_json::{Value, json};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, PinType, SchematicContent},
    symbol_library::{SymbolTemplate, TemplatePin},
    symbol_package::{PackageUnit, SchematicPackage, UnitRole},
};
use viamind_engine::{ChangeEvent, ChangeKind, Delta, EngineHandle, EventLog, Origin, SchematicConnectivityQuery, TxId};
use viamind_geom::Nm;

fn authored_template() -> viamind_doc::symbol_package::SymbolPackageTemplate {
    viamind_doc::symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json")).unwrap()
}
fn empty_engine() -> EngineHandle {
    let mut p = Project::new("Complete package");
    let mut sheet = Sheet::new("All units");
    sheet.content = Some(SchematicContent::default());
    p.schematic.sheets.insert(sheet.id, sheet);
    EngineHandle::new(p)
}
fn proposal(e: &EngineHandle) -> viamind_engine::PlaceSymbolPackage {
    let sheet_id = e.read(|p| *p.schematic.sheets.keys().next().unwrap());
    let guard = e.snapshot_symbol_sheet(&viamind_engine::SymbolSheetQuery { sheet_id }).unwrap();
    let template = authored_template();
    let placements = template
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| viamind_engine::UnitPose {
            key: u.key.clone(),
            position: Coordinate { x: Nm(i as i64 * 20_000_000 + 1), y: Nm(3) },
            rotation_quarters: (i % 4) as u8,
            mirrored: i % 2 == 1,
        })
        .collect();
    viamind_engine::PlaceSymbolPackage {
        expected_project: guard.expected_project,
        source_cursor: guard.source_cursor,
        expected: guard.expected,
        template,
        designator: "U1".into(),
        placements,
    }
}

#[test]
fn complete_original_five_unit_placement_is_one_delta_exact_mapping_native_log_and_scoped_undo() {
    for typed in [true, false] {
        let e = empty_engine();
        let before = e.session_state();
        let command = proposal(&e);
        let result = if typed {
            e.execute_typed(command.clone(), Origin::agent("creator"))
        } else {
            e.execute("place_symbol_package", serde_json::to_value(&command).unwrap(), Origin::agent("creator"))
        }
        .unwrap();
        assert_eq!(result.deltas.len(), 1);
        let placed = e.session_state();
        assert_eq!(placed.cursor.sequence, before.cursor.sequence + 1);
        let report = e.query_symbol_packages(&Default::default()).unwrap();
        assert_eq!(report.total, 1);
        let group = &report.packages[0];
        assert_eq!(group.units.iter().map(|u| u.key.as_str()).collect::<Vec<_>>(), ["A", "B", "C", "D", "P"]);
        assert_eq!(group.designator, "U1");
        let mut identities = std::collections::BTreeSet::from([group.package_id]);
        let mut numbers = std::collections::BTreeSet::new();
        for unit in &group.units {
            assert!(identities.insert(unit.symbol_id));
            let pose = command.placements.iter().find(|p| p.key == unit.key).unwrap();
            assert_eq!((unit.position, unit.rotation_quarters, unit.mirrored), (pose.position, pose.rotation_quarters, pose.mirrored));
            for p in &unit.pins {
                assert!(identities.insert(p.pin.id));
                assert!(numbers.insert(p.pin.number.clone()));
                // Independent signed cardinal oracle; no use of the model transform.
                let (mut x, mut y) = (p.pin.position.x.0, p.pin.position.y.0);
                if pose.mirrored {
                    x = -x;
                }
                (x, y) = match pose.rotation_quarters {
                    0 => (x, y),
                    1 => (y, -x),
                    2 => (-x, -y),
                    3 => (-y, x),
                    _ => unreachable!(),
                };
                assert_eq!(p.position, Coordinate { x: Nm(pose.position.x.0 + x), y: Nm(pose.position.y.0 + y) });
            }
        }
        assert_eq!(identities.len(), 20);
        assert_eq!(numbers, (1..=14).map(|i| i.to_string()).collect());
        assert_eq!(group.units[4].role, UnitRole::Power);
        assert_eq!(group.units[4].pins.len(), 2);
        let graph = e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: 0, limit: 100 }).unwrap();
        assert_eq!(graph.total_nets, 14);
        let erc = e.run_erc(&Default::default()).unwrap();
        assert_eq!(erc.coverage.evaluated_pins, 14);
        assert!(erc.violations.iter().flat_map(|v| &v.pins).any(|p| p.unit_key.as_deref() == Some("P") && p.electrical == Some(PinType::PowerInput)));
        assert_eq!(e.session_state(), placed, "read-only package/graph/ERC observations preserve every session field");
        assert_eq!(viamind_doc::file::minimum_version(&placed.project), 11);
        assert_eq!(viamind_doc::file::from_str(&viamind_doc::file::to_string(&placed.project)).unwrap(), placed.project);
        e.execute("add_part", json!({"designator":"PEER1","value":"keep","footprint":"F","x_mm":1,"y_mm":2}), Origin::agent("peer")).unwrap();
        let peer_board = e.read(|p| p.board.clone());
        e.undo_own(Origin::agent("creator")).unwrap();
        assert_eq!(e.read(|p| p.schematic.clone()), before.project.schematic);
        assert_eq!(e.read(|p| p.board.clone()), peer_board);
        e.redo_own(Origin::agent("creator")).unwrap();
        assert_eq!(e.read(|p| p.schematic.clone()), placed.project.schematic);
        let mut log = vec![];
        e.with_engine(|e| e.log().write_jsonl(&mut log)).unwrap();
        assert_eq!(EventLog::read_jsonl(log.as_slice()).unwrap().replay(), e.read(Clone::clone));
    }
}

#[test]
fn complete_deletion_keeps_wire_endpoints_and_restores_every_package_identity_on_undo() {
    let e = empty_engine();
    e.execute_typed(proposal(&e), Origin::User).unwrap();
    let sheet_id = e.read(|p| *p.schematic.sheets.keys().next().unwrap());
    let guard = e.snapshot_symbol_sheet(&viamind_engine::SymbolSheetQuery { sheet_id }).unwrap();
    e.execute("edit_schematic_sheet",json!({"expected_project":guard.expected_project,"sheet_id":sheet_id,"expected":guard.expected,"name":"All units","edits":[{"kind":"wire","start":{"x":-3000002,"y":-1000004},"end":{"x":9000000,"y":-1000004}}]}),Origin::User).unwrap();
    let before = e.session_state();
    let expected = before.project.schematic.sheets[&sheet_id].clone();
    let package_id = *expected.content.as_ref().unwrap().packages.keys().next().unwrap();
    let command = viamind_engine::DeleteSymbolPackage {
        expected_project: before.project.id,
        source_cursor: before.cursor,
        expected: expected.clone(),
        package_id,
    };
    assert_eq!(e.execute_typed(command.clone(), Origin::User).unwrap().deltas.len(), 1);
    let deleted = e.read(|p| p.schematic.sheets[&sheet_id].clone());
    let content = deleted.content.as_ref().unwrap();
    assert!(content.packages.is_empty() && content.symbols.is_empty());
    assert_eq!(content.wires, expected.content.as_ref().unwrap().wires);
    let after = e.session_state();
    assert!(e.execute_typed(command, Origin::User).is_err());
    assert_eq!(e.session_state(), after);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(|p| p.schematic.sheets[&sheet_id].clone()), expected);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(|p| p.schematic.sheets[&sheet_id].clone()), deleted);
}

#[test]
fn incomplete_duplicate_unknown_invalid_or_stale_placements_never_mutate_any_session_field() {
    for case in 0..10 {
        let e = empty_engine();
        let mut command = proposal(&e);
        match case {
            0 => {
                command.placements.pop();
            }
            1 => command.placements[4].key = "A".into(),
            2 => command.placements[4].key = "UNKNOWN".into(),
            3 => command.placements[0].rotation_quarters = 4,
            4 => command.placements[0].position.x = Nm(i64::MAX),
            5 => command.template.units[4].pins[0].number = "1".into(),
            6 => command.template.units[4].key = "A".into(),
            7 => command.expected.name = "forged".into(),
            8 => command.expected_project = ObjectId::new(),
            _ => command.template.value = "x".repeat(4097),
        }
        let before = e.session_state();
        let status = e.document_status();
        assert!(e.execute_typed(command, Origin::User).is_err(), "case{case}");
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
    }
    let e = empty_engine();
    let old = proposal(&e);
    e.execute("add_part", json!({"designator":"PEER1","value":"keep","footprint":"F","x_mm":0,"y_mm":0}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(old, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    let e = empty_engine();
    let old = proposal(&e);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.json");
    viamind_doc::file::save(&e.read(Clone::clone), &path).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: Some(e.event_cursor()) }, Origin::User).unwrap();
    let before = e.session_state();
    assert_eq!(before.project.id, old.expected_project);
    assert_eq!(before.project.schematic.sheets[&old.expected.id], old.expected);
    assert!(e.execute_typed(old, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    let e = empty_engine();
    let mut v = serde_json::to_value(proposal(&e)).unwrap();
    v["placements"] = json!(vec![Value::Null; 33]);
    let before = e.session_state();
    let error = e.execute("place_symbol_package", v, Origin::User).unwrap_err().to_string();
    assert!(error.contains("2..32"), "{error}");
    assert_eq!(e.session_state(), before);
}

fn package_sheet() -> Sheet {
    let mut sheet = Sheet::new("Original units");
    let mut c = SchematicContent::default();
    let id = ObjectId::new();
    let mut units = vec![];
    for (i, key) in ["A", "P"].into_iter().enumerate() {
        let template = SymbolTemplate {
            name: "Original package".into(),
            value: "Explicit pins".into(),
            footprint: None,
            width: Nm(4_000_003),
            height: Nm(2_000_007),
            pins: vec![TemplatePin {
                number: (i + 1).to_string(),
                name: key.into(),
                position: Coordinate { x: Nm(1), y: Nm(7) },
                electrical: PinType::Passive,
            }],
        };
        let symbol = template.instantiate("U1", Coordinate { x: Nm(i as i64 * 10_000_000), y: Nm(0) }, i as u8, false).unwrap();
        units.push(PackageUnit {
            key: key.into(),
            name: key.into(),
            role: if i == 0 { UnitRole::Functional } else { UnitRole::Power },
            symbol_id: symbol.id,
        });
        c.symbols.insert(symbol.id, symbol);
    }
    c.packages.insert(id, SchematicPackage { id, template_name: "Original package".into(), units });
    sheet.content = Some(c);
    sheet
}
fn bytes(base: Project, events: Vec<ChangeEvent>, version: u32) -> Vec<u8> {
    let mut log = EventLog::new(base);
    for event in events {
        log.append(event);
    }
    let mut bytes = vec![];
    log.write_jsonl(&mut bytes).unwrap();
    let lines = String::from_utf8(bytes).unwrap();
    let mut split = lines.splitn(2, '\n');
    let mut header: Value = serde_json::from_str(split.next().unwrap()).unwrap();
    header["version"] = json!(version);
    format!("{}\n{}", header, split.next().unwrap_or("")).into_bytes()
}
fn event(sheet: &Sheet, kind: ChangeKind, remove: bool) -> ChangeEvent {
    ChangeEvent {
        tx: TxId(1),
        origin: Origin::User,
        kind,
        deltas: vec![Delta::Sheet { id: sheet.id, before: remove.then(|| sheet.clone()), after: (!remove).then(|| sheet.clone()) }],
    }
}
#[test]
fn native11_log_roundtrip_preserves_group_identity_through_apply_undo_redo() {
    let base = Project::new("Packages");
    let sheet = package_sheet();
    let data = bytes(
        base.clone(),
        vec![event(&sheet, ChangeKind::Applied, false), event(&sheet, ChangeKind::Undone, true), event(&sheet, ChangeKind::Redone, false)],
        11,
    );
    let log = EventLog::read_jsonl(data.as_slice()).unwrap();
    assert_eq!(log.replay_until(2), base);
    assert_eq!(log.replay().schematic.sheets[&sheet.id], sheet);
    let mut again = vec![];
    log.write_jsonl(&mut again).unwrap();
    assert_eq!(EventLog::read_jsonl(again.as_slice()).unwrap(), log);
}
#[test]
fn old_log_headers_reject_groups_in_base_after_and_before_undo_snapshots() {
    let sheet = package_sheet();
    let mut grouped = Project::new("Grouped");
    grouped.schematic.sheets.insert(sheet.id, sheet.clone());
    assert!(EventLog::read_jsonl(bytes(grouped, vec![], 10).as_slice()).is_err());
    assert!(EventLog::read_jsonl(bytes(Project::new("After"), vec![event(&sheet, ChangeKind::Applied, false)], 10).as_slice()).is_err());
    // The current/base/result has no groups: the BEFORE-only historical shape must still reject.
    assert!(EventLog::read_jsonl(bytes(Project::new("Before"), vec![event(&sheet, ChangeKind::Undone, true)], 10).as_slice()).is_err());
    assert!(EventLog::read_jsonl(bytes(Project::new("Legacy"), vec![], 10).as_slice()).is_ok());
}
#[test]
fn explicit_group_reference_and_role_never_join_pins_and_graph_metadata_is_read_only() {
    let mut p = Project::new("Graph");
    let sheet = package_sheet();
    p.schematic.sheets.insert(sheet.id, sheet.clone());
    let e = EngineHandle::new(p);
    let before = e.session_state();
    let status = e.document_status();
    let graph = e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: 0, limit: 100 }).unwrap();
    assert_eq!(graph.total_nets, 2);
    let group = sheet.content.as_ref().unwrap().packages.values().next().unwrap();
    let mut keys = std::collections::BTreeSet::new();
    for net in &graph.nets {
        for m in &net.members {
            assert_eq!(m.package_id, Some(group.id));
            keys.insert(m.unit_key.as_deref().unwrap());
        }
    }
    assert_eq!(keys, std::collections::BTreeSet::from(["A", "P"]));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
#[test]
fn generic_unit_pose_is_atomic_but_body_and_partial_delete_reject_without_history() {
    let mut p = Project::new("Pose");
    let sheet = package_sheet();
    p.schematic.sheets.insert(sheet.id, sheet.clone());
    let e = EngineHandle::new(p);
    let before = e.session_state();
    let symbol = sheet.content.as_ref().unwrap().symbols.values().next().unwrap();
    for edit in [json!({"kind":"delete","id":symbol.id}), {
        let mut s = serde_json::to_value(symbol).unwrap();
        s["value"] = json!("forbidden");
        for pin in s["pins"].as_array_mut().unwrap() {
            pin.as_object_mut().unwrap().remove("id");
        }
        json!({"kind":"symbol","symbol":s})
    }] {
        assert!(
            e.execute(
                "edit_schematic_sheet",
                json!({"expected_project":before.project.id,"sheet_id":sheet.id,"expected":sheet,"name":sheet.name,"edits":[edit]}),
                Origin::User
            )
            .is_err()
        );
        assert_eq!(e.session_state(), before);
    }
    let mut s = serde_json::to_value(symbol).unwrap();
    s["position"] = json!({"x":123,"y":456});
    s["rotation_quarters"] = json!(3);
    for pin in s["pins"].as_array_mut().unwrap() {
        pin.as_object_mut().unwrap().remove("id");
    }
    let r=e.execute("edit_schematic_sheet",json!({"expected_project":before.project.id,"sheet_id":sheet.id,"expected":sheet,"name":sheet.name,"edits":[{"kind":"symbol","symbol":s}]}),Origin::User).unwrap();
    assert_eq!(r.deltas.len(), 1);
    assert_eq!(e.read(|p| p.schematic.sheets[&sheet.id].content.as_ref().unwrap().packages.clone()), sheet.content.as_ref().unwrap().packages);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}

#[test]
fn package_membership_is_charged_before_selected_snapshot_and_graph_copies() {
    let mut p = Project::new("Bounded");
    let mut sheet = package_sheet();
    let c = sheet.content.as_mut().unwrap();
    let prototype = c.packages.values().next().unwrap().clone();
    // 3334 groups + 6668 member records exceed 10000 before validation can
    // encounter the intentionally duplicated member ownership.
    c.packages.clear();
    for _ in 0..3334 {
        let mut group = prototype.clone();
        group.id = ObjectId::new();
        c.packages.insert(group.id, group);
    }
    p.schematic.sheets.insert(sheet.id, sheet.clone());
    let e = EngineHandle::new(p);
    let before = e.session_state();
    let error = e.snapshot_symbol_sheet(&viamind_engine::SymbolSheetQuery { sheet_id: sheet.id }).unwrap_err().to_string();
    assert!(error.contains("10000"), "{error}");
    let error = e.query_schematic_connectivity(&SchematicConnectivityQuery { offset: 0, limit: 100 }).unwrap_err().to_string();
    assert!(error.contains("10000"), "{error}");
    assert_eq!(e.session_state(), before);
}

#[test]
fn flat_annotation_and_whole_pcb_sync_refuse_grouped_projects_without_mutation() {
    let mut p = Project::new("Explicit integration gap");
    let sheet = package_sheet();
    p.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(p);
    let before = e.session_state();
    assert!(e.preview_schematic_annotation(Default::default()).unwrap_err().to_string().contains("multi-unit package"));
    assert!(e.preview_schematic_pcb_sync(viamind_engine::PcbSyncDirection::Forward).unwrap_err().to_string().contains("multi-unit package"));
    assert_eq!(e.session_state(), before);
}

#[test]
fn new_package_command_sheet_guards_admit_borrowed_counts_and_text_before_poison_rows() {
    let e = empty_engine();
    let original = e.session_state();
    let status = e.document_status();
    let base = serde_json::to_value(proposal(&e)).unwrap();
    let map = (0..10001).map(|i| (format!("malformed-{i}"), Value::Null)).collect::<serde_json::Map<_, _>>();
    let cases = [
        (json!({"symbols":map}), "10000 sheet objects"),
        (json!({"symbols":{"invalid":{"pins":vec![Value::Null;10001]}}}), "10000 sheet pins"),
        (json!({"packages":{"invalid":{"units":vec![Value::Null;10001]}}}), "10000 objects/package members"),
        (json!({"symbols":{"invalid":{"value":"é".repeat(2049)}}}), "4096bytes/string"),
        (json!({"symbols":(0..513).map(|i|(i.to_string(),json!({"value":"x".repeat(4096)}))).collect::<serde_json::Map<_,_>>()}), "2MiB"),
    ];
    for (content, needle) in cases {
        let mut guard = base["expected"].clone();
        guard["content"] = content;
        let mut place = base.clone();
        place["expected"] = guard.clone();
        let delete =
            json!({"expected_project":base["expected_project"],"source_cursor":base["source_cursor"],"expected":guard,"package_id":ObjectId::new()});
        for (name, input) in [("place_symbol_package", place), ("delete_symbol_package", delete)] {
            let error = e.execute(name, input, Origin::agent("adversarial guard")).unwrap_err();
            assert!(error.to_string().contains(needle), "{name}: {error}");
            assert_eq!(e.session_state(), original);
            assert_eq!(e.document_status(), status);
        }
    }
}
