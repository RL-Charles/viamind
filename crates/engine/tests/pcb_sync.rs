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
