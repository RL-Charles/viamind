use serde_json::json;
use viamind_doc::{
    ObjectId, Project, Sheet, file,
    schematic::{Coordinate, PinType, SchematicContent},
    symbol_library::{SymbolTemplate, TemplatePin},
};
use viamind_engine::{Command, EngineHandle, EventLog, ExportSymbolTemplate, LoadSymbolTemplate, Origin, PlaceSymbolTemplate, SymbolSheetQuery};
use viamind_geom::Nm;

fn fixture() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
}
fn command(engine: &EngineHandle) -> PlaceSymbolTemplate {
    let sheet_id = engine.read(|p| *p.schematic.sheets.keys().next().unwrap());
    let guard = engine.snapshot_symbol_sheet(&SymbolSheetQuery { sheet_id }).unwrap();
    PlaceSymbolTemplate {
        expected_project: guard.expected_project,
        source_cursor: guard.source_cursor,
        expected: guard.expected,
        template: SymbolTemplate {
            name: "Original sensor".into(),
            value: "Sensor".into(),
            footprint: Some("Original-SMD".into()),
            width: Nm(1_000_001),
            height: Nm(2_000_003),
            pins: vec![TemplatePin {
                number: "1".into(),
                name: "OUT".into(),
                position: Coordinate { x: Nm(3), y: Nm(5) },
                electrical: PinType::Output,
            }],
        },
        designator: "S1".into(),
        position: Coordinate { x: Nm(50_000_001), y: Nm(60_000_003) },
        rotation_quarters: 1,
        mirrored: true,
    }
}

#[test]
fn typed_and_dynamic_creation_are_one_event_with_exact_fresh_pins_and_replayable_undo() {
    for typed in [true, false] {
        let e = fixture();
        let before = e.session_state();
        let cmd = command(&e);
        assert_eq!(e.session_state(), before);
        if typed {
            e.execute_typed(cmd, Origin::agent("library-creator")).unwrap();
        } else {
            e.execute(PlaceSymbolTemplate::NAME, serde_json::to_value(cmd).unwrap(), Origin::agent("library-creator")).unwrap();
        }
        let after = e.session_state();
        assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
        assert_eq!(after.project.board, before.project.board);
        let sheet = after.project.schematic.sheets.values().next().unwrap();
        let created = sheet.content.as_ref().unwrap().symbols.values().find(|s| s.designator == "S1").unwrap();
        assert_eq!(created.pin_position(created.pins[0].position).unwrap(), Coordinate { x: Nm(50_000_006), y: Nm(60_000_006) });
        assert!(!created.id.as_uuid().is_nil());
        assert!(!created.pins[0].id.as_uuid().is_nil());
        assert_ne!(created.id, created.pins[0].id);
        let mut restored = after.project.clone();
        restored.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.remove(&created.id);
        assert_eq!(restored, before.project);
        assert_eq!(file::from_str(&file::to_string(&after.project)).unwrap(), after.project);
        let mut log = vec![];
        e.with_engine(|e| e.log().write_jsonl(&mut log)).unwrap();
        assert_eq!(EventLog::read_jsonl(&log[..]).unwrap().replay(), after.project);
        e.execute("add_part", json!({"designator":"R_PEER","value":"keep","footprint":"F","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
        let peer = e.read(|p| p.board.clone());
        e.undo(Origin::agent("library-creator")).unwrap();
        assert_eq!(e.read(|p| p.schematic.clone()), before.project.schematic);
        assert_eq!(e.read(|p| p.board.clone()), peer);
        e.redo(Origin::agent("library-creator")).unwrap();
        assert_eq!(e.read(|p| p.schematic.clone()), after.project.schematic);
        assert_eq!(e.read(|p| p.board.clone()), peer);
    }
}

#[test]
fn malformed_ambiguous_stale_and_reopened_proposals_leave_entire_sessions_unchanged() {
    for kind in 0..8 {
        let e = fixture();
        let mut cmd = command(&e);
        match kind {
            0 => cmd.rotation_quarters = 4,
            1 => cmd.position.x = Nm(i64::MAX),
            2 => cmd.template.pins.push(cmd.template.pins[0].clone()),
            3 => cmd.designator = "U1".into(),
            4 => cmd.expected.name = "forged".into(),
            5 => cmd.expected_project = ObjectId::new(),
            6 => cmd.template.value = "x".repeat(4097),
            _ => cmd.position.x = Nm(10_000_000_000),
        }
        let before = e.session_state();
        let status = e.document_status();
        assert!(e.execute_typed(cmd, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
    }
    let e = fixture();
    let old = command(&e);
    e.execute("add_part", json!({"designator":"R_PEER","value":"keep","footprint":"F","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    assert!(e.execute_typed(old, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    let e = fixture();
    let old = command(&e);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.viamind.json");
    file::save(&e.read(Clone::clone), &path).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: Some(e.event_cursor()) }, Origin::User).unwrap();
    let reopened = e.session_state();
    assert_eq!(reopened.project.id, old.expected_project);
    assert_ne!(reopened.cursor.stream_id, old.source_cursor.stream_id);
    assert!(e.execute_typed(old, Origin::User).is_err());
    assert_eq!(e.session_state(), reopened);
}

#[test]
fn selected_and_global_preflight_bound_unrelated_data_before_inherited_validation() {
    let mut p = fixture().read(Clone::clone);
    let mut sheet = Sheet::new("other");
    sheet.content = Some(SchematicContent::default());
    // Unrelated excessive text must not be copied into the selected-sheet guard.
    sheet.name = "x".repeat(4097);
    p.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(p);
    let target = e.read(|p| p.schematic.sheets.values().find(|s| s.name == "Power regulator").unwrap().id);
    let guard = e.snapshot_symbol_sheet(&SymbolSheetQuery { sheet_id: target }).unwrap();
    let mut cmd = command(&fixture());
    cmd.expected_project = guard.expected_project;
    cmd.source_cursor = guard.source_cursor;
    cmd.expected = guard.expected;
    let before = e.session_state();
    assert!(e.execute_typed(cmd, Origin::User).unwrap_err().to_string().contains("global symbol validation text"));
    assert_eq!(e.session_state(), before);
    let e = fixture();
    let mut cmd = command(&e);
    cmd.expected.name = "x".repeat(4097);
    let before = e.session_state();
    assert!(e.execute_typed(cmd, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    let mut p = Project::new("too many sheets");
    for _ in 0..1001 {
        let s = Sheet::new("empty");
        p.schematic.sheets.insert(s.id, s);
    }
    let e = EngineHandle::new(p);
    let cmd = command(&e);
    let before = e.session_state();
    assert!(e.execute_typed(cmd, Origin::User).unwrap_err().to_string().contains("1000 sheets"));
    assert_eq!(e.session_state(), before);
}

#[test]
fn source_only_export_is_guarded_detached_new_and_does_not_inspect_unrelated_content() {
    let mut project = fixture().read(Clone::clone);
    let unrelated = Sheet::new("x".repeat(4097));
    project.schematic.sheets.insert(unrelated.id, unrelated);
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let sheet = before.project.schematic.sheets.values().find(|s| s.name == "Power regulator").unwrap();
    let symbol = sheet.content.as_ref().unwrap().symbols.values().find(|s| s.designator == "U1").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("regulator.symbol.json");
    let input = ExportSymbolTemplate {
        expected_project: before.project.id,
        source_cursor: before.cursor,
        sheet_id: sheet.id,
        symbol_id: symbol.id,
        name: "Original regulator".into(),
        path: path.clone(),
    };
    let result = e.export_symbol_template(input.clone()).unwrap();
    assert_eq!(result.exported_cursor, before.cursor);
    assert_eq!(result.pin_count, 3);
    assert_eq!(e.session_state(), before);
    let bytes = std::fs::read(&path).unwrap();
    let loaded = e.load_symbol_template(LoadSymbolTemplate { path: path.clone() }).unwrap();
    assert_eq!(loaded.pins.len(), 3);
    assert_eq!(loaded.value, "3.3 V regulator");
    assert!(!String::from_utf8(bytes.clone()).unwrap().contains(&symbol.id.to_string()));
    assert!(e.export_symbol_template(input.clone()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    e.execute("add_part", json!({"designator":"R_PEER","value":"keep","footprint":"F","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    let changed = e.session_state();
    let mut stale = input;
    stale.path = directory.path().join("stale.json");
    assert!(e.export_symbol_template(stale).is_err());
    assert!(!directory.path().join("stale.json").exists());
    assert_eq!(e.session_state(), changed);
}

#[test]
fn proposed_fresh_symbol_and_pin_are_included_in_global_identity_budget() {
    let mut project = Project::new("full identity budget");
    let sheet = Sheet::new("empty");
    project.schematic.sheets.insert(sheet.id, sheet);
    for _ in 0..99_998 {
        let via = viamind_doc::Via {
            id: ObjectId::new(),
            position: viamind_geom::Point::new(Nm(0), Nm(0)),
            diameter: Nm(600_000),
            drill: Nm(300_000),
            net: None,
        };
        project.board.vias.insert(via.id, via);
    }
    let engine = EngineHandle::new(project);
    let proposal = command(&engine);
    let before = engine.session_state();
    assert!(engine.execute_typed(proposal, Origin::User).unwrap_err().to_string().contains("100000 combined"));
    assert_eq!(engine.session_state(), before);
}
