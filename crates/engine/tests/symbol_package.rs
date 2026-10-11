use serde_json::{Value, json};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, PinType, SchematicContent},
    symbol_library::{SymbolTemplate, TemplatePin},
    symbol_package::{PackageUnit, SchematicPackage, UnitRole},
};
use viamind_engine::{ChangeEvent, ChangeKind, Delta, EngineHandle, EventLog, Origin, SchematicConnectivityQuery, TxId};
use viamind_geom::Nm;

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
