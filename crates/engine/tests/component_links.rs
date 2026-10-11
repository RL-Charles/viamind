use serde_json::json;
use viamind_doc::{ObjectId, file};
use viamind_engine::{ComponentLinkQuery, ComponentLinkStatus, EngineHandle, Origin};
fn engine() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap())
}
#[test]
fn stable_mapping_pages_filters_and_reference_renames_preserve_exact_session() {
    let e = engine();
    let before = e.session_state();
    let report = e.query_linked_components(&ComponentLinkQuery::default()).unwrap();
    assert_eq!(report.total, 3);
    assert_eq!(report.components.len(), 3);
    assert!(report.components.iter().all(|row| row.status == ComponentLinkStatus::Linked));
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(e.query_linked_components(&ComponentLinkQuery::default()).unwrap()).unwrap()
    );
    assert_eq!(e.session_state(), before);
    assert_eq!(report.source_cursor, before.cursor);
    let first = &report.components[0];
    let one = e.query_linked_components(&ComponentLinkQuery { limit: Some(1), ..Default::default() }).unwrap();
    assert_eq!(one.next_offset, Some(1));
    let last = e.query_linked_components(&ComponentLinkQuery { offset: 2, limit: Some(1), ..Default::default() }).unwrap();
    assert_eq!(last.next_offset, None);
    assert_eq!(last.components[0].part_id, report.components[2].part_id);
    let missing = ObjectId::new();
    let filtered = e
        .query_linked_components(&ComponentLinkQuery {
            object_ids: vec![first.part_id.unwrap(), first.symbol_id.unwrap(), missing],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.unknown_ids, vec![missing]);
    e.execute("update_part", json!({"id":first.part_id,"designator":"RENAMED1"}), Origin::User).unwrap();
    let renamed = e.query_linked_components(&ComponentLinkQuery { object_ids: vec![first.symbol_id.unwrap()], ..Default::default() }).unwrap();
    assert_eq!(renamed.components[0].part_id, first.part_id);
    assert_eq!(renamed.components[0].part_reference.as_deref(), Some("RENAMED1"));
    assert_ne!(renamed.source_cursor, report.source_cursor);
}
#[test]
fn unresolved_unlinked_and_extreme_coordinates_are_explicit_without_float_conversion() {
    let mut p = engine().read(Clone::clone);
    let part = p.board.parts.values_mut().next().unwrap();
    let symbol = part.schematic_symbol.unwrap();
    let part_id = part.id;
    part.position.x.0 = i64::MAX;
    p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.remove(&symbol);
    let e = EngineHandle::new(p);
    let report = e.query_linked_components(&ComponentLinkQuery { object_ids: vec![part_id, symbol], ..Default::default() }).unwrap();
    assert_eq!(report.components.len(), 1);
    let row = &report.components[0];
    assert_eq!(row.status, ComponentLinkStatus::UnresolvedSymbol);
    assert!(!row.board_navigable && !row.schematic_navigable);
    assert_eq!(row.board_position.unwrap().x.0, i64::MAX);
    assert_eq!(report.unknown_ids, vec![symbol]);
    let mut p = engine().read(Clone::clone);
    p.board.parts.values_mut().next().unwrap().schematic_symbol = None;
    let report = EngineHandle::new(p).query_linked_components(&ComponentLinkQuery::default()).unwrap();
    assert_eq!(report.total, 4);
    assert_eq!(report.components.iter().filter(|r| r.status == ComponentLinkStatus::UnlinkedPart).count(), 1);
    assert_eq!(report.components.iter().filter(|r| r.status == ComponentLinkStatus::UnlinkedSymbol).count(), 1);
}
#[test]
fn invalid_queries_identities_text_and_object_bounds_reject_without_mutation() {
    let e = engine();
    let before = e.session_state();
    for input in [json!({"limit":0}), json!({"limit":101}), json!({"object_ids":vec![ObjectId::new();101]})] {
        assert!(e.query_linked_components(&serde_json::from_value(input).unwrap()).is_err());
        assert_eq!(e.session_state(), before);
    }
    let id = e.read(|p| *p.board.parts.keys().next().unwrap());
    assert!(e.query_linked_components(&ComponentLinkQuery { object_ids: vec![id, id], ..Default::default() }).is_err());
    for case in 0..5 {
        let mut p = e.read(Clone::clone);
        match case {
            0 => p.board.parts.values_mut().next().unwrap().designator = "a".repeat(4097),
            1 => p.board.parts.values_mut().next().unwrap().schematic_symbol = Some(p.id),
            2 => {
                let link = p.board.parts.values().next().unwrap().schematic_symbol;
                p.board.parts.values_mut().nth(1).unwrap().schematic_symbol = link;
            }
            3 => p.board.parts.values_mut().next().unwrap().id = ObjectId::new(),
            _ => {
                let template = p.board.parts.values().next().unwrap().clone();
                for _ in 0..10001 {
                    let mut part = template.clone();
                    part.id = ObjectId::new();
                    p.board.parts.insert(part.id, part);
                }
            }
        }
        assert!(EngineHandle::new(p).query_linked_components(&ComponentLinkQuery::default()).is_err(), "case {case}");
    }
}
