use serde_json::json;
use viamind_doc::{ObjectId, Side, file};
use viamind_engine::{EngineHandle, Origin, PinPadCoverage, PinPadLinkQuery};
use viamind_geom::Nm;
fn engine() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap())
}
fn query(e: &EngineHandle) -> PinPadLinkQuery {
    PinPadLinkQuery { component_id: e.read(|p| *p.board.parts.keys().next().unwrap()), offset: 0, limit: None }
}
#[test]
fn deterministic_pages_exact_pairing_and_read_only_session_survive_reference_and_net_changes() {
    let e = engine();
    let q = query(&e);
    let before = e.session_state();
    let status = e.document_status();
    let r = e.query_linked_pin_pads(&q).unwrap();
    assert_eq!(r.coverage, PinPadCoverage::Linked);
    assert_eq!(r.total, 3);
    assert!(r.terminals.iter().all(|p| p.navigable && p.pin_id.is_some() && p.pad_id.is_some()));
    assert_eq!(serde_json::to_value(&r).unwrap(), serde_json::to_value(e.query_linked_pin_pads(&q).unwrap()).unwrap());
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    let page = e.query_linked_pin_pads(&PinPadLinkQuery { limit: Some(1), ..q.clone() }).unwrap();
    assert_eq!(page.next_offset, Some(1));
    assert_eq!(page.terminals, r.terminals[..1]);
    let last = e.query_linked_pin_pads(&PinPadLinkQuery { offset: 2, limit: Some(1), ..q.clone() }).unwrap();
    assert_eq!(last.next_offset, None);
    assert_eq!(last.terminals, r.terminals[2..]);
    e.execute("update_part", json!({"id":q.component_id,"designator":"DIFFERENT9"}), Origin::User).unwrap();
    let renamed = e.query_linked_pin_pads(&q).unwrap();
    assert_eq!(renamed.terminals, r.terminals);
    assert_ne!(renamed.source_cursor, r.source_cursor);
    let mut p = e.read(Clone::clone);
    for pad in &mut p.board.parts.get_mut(&q.component_id).unwrap().pads.as_mut().unwrap().0 {
        pad.net = None;
    }
    let e = EngineHandle::new(p);
    assert_eq!(e.query_linked_pin_pads(&q).unwrap().terminals, r.terminals, "nets never establish physical mappings");
    assert_ne!(e.query_linked_pin_pads(&q).unwrap().source_cursor.stream_id, r.source_cursor.stream_id);
}
#[test]
fn exact_odd_nanometre_mirrored_quarter_turn_geometry_and_honest_unsupported_centers() {
    let e = engine();
    let q = query(&e);
    let mut p = e.read(Clone::clone);
    let part = p.board.parts.get_mut(&q.component_id).unwrap();
    part.side = Side::Bottom;
    part.rotation_mdeg = 90_000;
    part.position.x = Nm(101);
    part.position.y = Nm(203);
    let pad = &mut part.pads.as_mut().unwrap().0[0];
    pad.position.x = Nm(7);
    pad.position.y = Nm(-11);
    let id = pad.id;
    let e = EngineHandle::new(p);
    let r = e.query_linked_pin_pads(&q).unwrap();
    let row = r.terminals.iter().find(|p| p.pad_id == Some(id)).unwrap();
    assert_eq!(row.board_position.unwrap().x, Nm(90));
    assert_eq!(row.board_position.unwrap().y, Nm(210));
    for position in [0, i64::MAX] {
        let mut p = e.read(Clone::clone);
        let part = p.board.parts.get_mut(&q.component_id).unwrap();
        part.rotation_mdeg = 45_000;
        part.position.x = Nm(position);
        let r = EngineHandle::new(p).query_linked_pin_pads(&q).unwrap();
        assert!(r.terminals.iter().all(|p| !p.navigable && p.board_position.is_none() && !p.unsupported_reasons.is_empty()));
    }
}
#[test]
fn mismatch_legacy_unlinked_and_empty_coverage_never_offer_partial_navigation() {
    let e = engine();
    let q = query(&e);
    for case in 0..4 {
        let mut p = e.read(Clone::clone);
        let part = p.board.parts.get_mut(&q.component_id).unwrap();
        let symbol = part.schematic_symbol.unwrap();
        match case {
            0 => part.pads.as_mut().unwrap().0[0].number = "EXTRA".into(),
            1 => part.pads = None,
            2 => part.schematic_symbol = None,
            _ => {
                part.pads.as_mut().unwrap().0.clear();
                p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.get_mut(&symbol).unwrap().pins.clear();
            }
        }
        let e = EngineHandle::new(p);
        let before = e.session_state();
        let r = e.query_linked_pin_pads(&q).unwrap();
        assert_eq!(
            r.coverage,
            [PinPadCoverage::TerminalMismatch, PinPadCoverage::NoExplicitPads, PinPadCoverage::Unlinked, PinPadCoverage::EmptyTerminals][case]
        );
        assert!(r.terminals.iter().all(|p| !p.navigable));
        assert_eq!(e.session_state(), before);
    }
}
#[test]
fn malformed_identities_numbers_queries_and_unrelated_structure_limits_reject_unchanged() {
    let e = engine();
    let q = query(&e);
    for case in 0..6 {
        let mut p = e.read(Clone::clone);
        match case {
            0 => {
                let pads = &mut p.board.parts.get_mut(&q.component_id).unwrap().pads.as_mut().unwrap().0;
                pads[0].id = pads[1].id;
            }
            1 => {
                let pads = &mut p.board.parts.get_mut(&q.component_id).unwrap().pads.as_mut().unwrap().0;
                pads[0].number = pads[1].number.clone();
            }
            2 => {
                p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.values_mut().next().unwrap().pins[0].name =
                    "x".repeat(4097)
            }
            3 => {
                let template = p.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().symbols.values().next().unwrap().pins[0].clone();
                p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.values_mut().next().unwrap().pins =
                    vec![template; 10001];
            }
            4 => p.board.parts.get_mut(&q.component_id).unwrap().pads.as_mut().unwrap().0[0].number = " ".into(),
            _ => p.board.parts.get_mut(&q.component_id).unwrap().pads.as_mut().unwrap().0[0].id = p.id,
        }
        let e = EngineHandle::new(p);
        let before = e.session_state();
        assert!(e.query_linked_pin_pads(&q).is_err(), "case {case}");
        assert_eq!(e.session_state(), before);
    }
    for q in [
        PinPadLinkQuery { component_id: ObjectId::new(), ..q.clone() },
        PinPadLinkQuery { limit: Some(0), ..q.clone() },
        PinPadLinkQuery { limit: Some(101), ..q.clone() },
    ] {
        assert!(e.query_linked_pin_pads(&q).is_err());
    }
    assert!(serde_json::from_value::<PinPadLinkQuery>(json!({"component_id":q.component_id,"unexpected":true})).is_err());
}
