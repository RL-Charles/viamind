use serde_json::json;
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, PinType, SchematicContent, SchematicSymbol, SymbolPin},
};
use viamind_engine::{AnnotationMode, AnnotationOptions, AnnotationOrder, EngineHandle, EventLog, Origin};
use viamind_geom::Nm;
fn id(n: u64) -> ObjectId {
    format!("00000000-0000-0000-0000-{n:012x}").parse().unwrap()
}
fn symbol(n: u64, reference: &str, x: i64, y: i64) -> SchematicSymbol {
    SchematicSymbol {
        id: id(n),
        designator: reference.into(),
        value: "Value retained".into(),
        footprint: Some("OriginalSMD".into()),
        position: Coordinate { x: Nm(x), y: Nm(y) },
        rotation_quarters: 3,
        mirrored: true,
        width: Nm(4_000_000),
        height: Nm(6_000_000),
        pins: vec![SymbolPin {
            id: id(n + 1000),
            number: "1".into(),
            name: "PIN".into(),
            position: Coordinate { x: Nm(-3_000_000), y: Nm(0) },
            electrical: PinType::Passive,
        }],
    }
}
fn project() -> Project {
    let mut p = Project::new("Annotation");
    for (sheet_id, symbols) in [
        (id(1), vec![symbol(11, "R8", 20, 0), symbol(12, "R9", 0, 20), symbol(13, "C8", 5, 10), symbol(14, "R7", 0, 20)]),
        (id(2), vec![symbol(21, "R6", -20, -20), symbol(22, "C7", 0, 0)]),
    ] {
        let mut sheet = Sheet::new("Sheet");
        sheet.id = sheet_id;
        sheet.content = Some(SchematicContent { symbols: symbols.into_iter().map(|s| (s.id, s)).collect(), ..Default::default() });
        p.schematic.sheets.insert(sheet_id, sheet);
    }
    p
}
fn refs(p: &Project) -> Vec<String> {
    [11, 12, 13, 14, 21, 22]
        .into_iter()
        .map(|n| p.schematic.sheets.values().find_map(|s| s.content.as_ref()?.symbols.get(&id(n))).unwrap().designator.clone())
        .collect()
}
fn options() -> AnnotationOptions {
    AnnotationOptions { mode: AnnotationMode::ResetAll, ..Default::default() }
}
#[test]
fn deterministic_explicit_sheet_position_ties_and_one_undo_preserve_every_other_field() {
    let p = project();
    let e = EngineHandle::new(p.clone());
    let before = e.session_state();
    let events = e.subscribe();
    let preview = e.preview_schematic_annotation(options()).unwrap();
    assert_eq!(e.session_state(), before);
    assert_eq!(preview.sheet_order, vec![id(1), id(2)]);
    assert_eq!(preview.symbols, 6);
    e.execute("annotate_schematic", json!(preview.command), Origin::agent("a")).unwrap();
    let changed = e.read(Clone::clone);
    assert_eq!(refs(&changed), ["R3", "R1", "C1", "R2", "R4", "C2"]);
    let mut restored_fields = changed.clone();
    for (sid, sheet) in &mut restored_fields.schematic.sheets {
        for (oid, s) in &mut sheet.content.as_mut().unwrap().symbols {
            s.designator = p.schematic.sheets[sid].content.as_ref().unwrap().symbols[oid].designator.clone();
        }
    }
    assert_eq!(restored_fields, p);
    let mut log = EventLog::new(p.clone());
    log.append(events.recv().unwrap());
    assert_eq!(log.replay(), changed);
    let mut bytes = vec![];
    log.write_jsonl(&mut bytes).unwrap();
    assert_eq!(EventLog::read_jsonl(std::io::Cursor::new(bytes)).unwrap().replay(), changed);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("annotated.viamind.json");
    viamind_doc::file::save(&changed, &path).unwrap();
    assert_eq!(viamind_doc::file::load(&path).unwrap(), changed);
    e.undo_own(Origin::agent("a")).unwrap();
    assert_eq!(e.read(Clone::clone), p);
    e.redo_own(Origin::agent("a")).unwrap();
    assert_eq!(e.read(Clone::clone), changed);
    let same = e.preview_schematic_annotation(options()).unwrap();
    assert!(same.changes.is_empty());
    let state = e.session_state();
    e.execute("annotate_schematic", json!(same.command), Origin::agent("a")).unwrap();
    assert_eq!(e.session_state(), state);
    let other = EngineHandle::new(p);
    let preview = other
        .preview_schematic_annotation(AnnotationOptions { sheet_order: vec![id(2), id(1)], order: AnnotationOrder::YThenX, start: 10, ..options() })
        .unwrap();
    other.execute("annotate_schematic", json!(preview.command), Origin::User).unwrap();
    assert_eq!(refs(&other.read(Clone::clone)), ["R11", "R12", "C11", "R13", "R10", "C10"]);
}
#[test]
fn keep_existing_reserves_gaps_case_and_numeric_aliases_and_exhaustion_are_atomic() {
    let mut p = project();
    let s = &mut p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap().symbols;
    s.get_mut(&id(11)).unwrap().designator = "r01".into();
    s.get_mut(&id(12)).unwrap().designator = "R?".into();
    s.get_mut(&id(14)).unwrap().designator = "R3".into();
    let e = EngineHandle::new(p.clone());
    let preview = e.preview_schematic_annotation(AnnotationOptions::default()).unwrap();
    assert_eq!(preview.changes.len(), 1);
    assert_eq!(preview.changes[0].after, "R2");
    e.execute("annotate_schematic", json!(preview.command), Origin::User).unwrap();
    assert_eq!(refs(&e.read(Clone::clone)), ["r01", "R2", "C8", "R3", "R6", "C7"]);
    let s = &mut p.schematic.sheets.get_mut(&id(2)).unwrap().content.as_mut().unwrap().symbols;
    s.get_mut(&id(21)).unwrap().designator = "R1".into();
    let bad = EngineHandle::new(p);
    let before = bad.session_state();
    assert!(bad.preview_schematic_annotation(AnnotationOptions::default()).unwrap_err().to_string().contains("ambiguous"));
    assert_eq!(bad.session_state(), before);
    assert!(bad.preview_schematic_annotation(options()).is_ok());
    let exhausted = EngineHandle::new(project());
    let before = exhausted.session_state();
    assert!(exhausted.preview_schematic_annotation(AnnotationOptions { start: 999_999_999, ..options() }).is_err());
    assert_eq!(exhausted.session_state(), before);
}
#[test]
fn stale_sheets_invalid_formats_orders_and_budgets_leave_whole_session_unchanged() {
    let e = EngineHandle::new(project());
    let preview = e.preview_schematic_annotation(options()).unwrap();
    let sheet = e.read(|p| p.schematic.sheets[&id(1)].clone());
    e.execute(
        "edit_schematic_sheet",
        json!({"expected_project":preview.command.expected_project,"sheet_id":sheet.id,"expected":sheet,"name":"Changed","edits":[]}),
        Origin::agent("other"),
    )
    .unwrap();
    let before = e.session_state();
    assert!(e.execute("annotate_schematic", json!(preview.command), Origin::agent("a")).is_err());
    assert_eq!(e.session_state(), before);
    for order in [vec![id(1)], vec![id(1), id(1)], vec![id(1), id(9)]] {
        assert!(e.preview_schematic_annotation(AnnotationOptions { sheet_order: order, ..options() }).is_err());
        assert_eq!(e.session_state(), before);
    }
    for name in ["#PWR1", "R1A", "R0", "R-1", "R1000000000", "é1", "ABCDEFGHIJKLMNOPQ1"] {
        let mut p = project();
        p.schematic.sheets.get_mut(&id(1)).unwrap().content.as_mut().unwrap().symbols.get_mut(&id(11)).unwrap().designator = name.into();
        let e = EngineHandle::new(p);
        let state = e.session_state();
        assert!(e.preview_schematic_annotation(options()).is_err(), "{name}");
        assert_eq!(e.session_state(), state);
    }
    let mut p = project();
    for n in 100..201 {
        let mut s = Sheet::new("Extra");
        s.id = id(n);
        p.schematic.sheets.insert(s.id, s);
    }
    let e = EngineHandle::new(p);
    let state = e.session_state();
    assert!(e.preview_schematic_annotation(options()).unwrap_err().to_string().contains("100 sheets"));
    assert_eq!(e.session_state(), state);
}
#[test]
fn board_only_edits_do_not_invalidate_annotation_or_enter_its_undo() {
    let e = EngineHandle::new(project());
    let preview = e.preview_schematic_annotation(options()).unwrap();
    e.execute("add_part", json!({"designator":"U1","value":"Board","footprint":"SMD","x_mm":1.0,"y_mm":2.0}), Origin::agent("board")).unwrap();
    let board = e.read(|p| p.board.clone());
    e.execute("annotate_schematic", json!(preview.command), Origin::agent("annotation")).unwrap();
    e.undo_own(Origin::agent("annotation")).unwrap();
    assert_eq!(e.read(|p| p.board.clone()), board);
    assert_eq!(refs(&e.read(Clone::clone)), refs(&project()));
}
