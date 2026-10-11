//! Actual component selection/navigation inputs against the production desktop.
use egui::{Event, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{ObjectId, file};
use viamind_engine::{ComponentLinkQuery, EngineHandle, Origin, SelectionMode, SetSelection};
use viamind_ui_egui::{ViamindApp, camera::WorldPos, layers::LayerId};
fn engine() -> EngineHandle {
    EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap())
}
fn harness(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 2000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn settle(h: &mut Harness<'_, ViamindApp>, label: &str) {
    for _ in 0..300 {
        h.run_steps(2);
        if h.query_by_label(label).is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("missing {label}");
}
fn select_board(h: &mut Harness<'_, ViamindApp>, at: viamind_geom::Point) {
    let p = h.state().canvas.camera.world_to_screen(WorldPos::new(at.x.to_mm(), at.y.to_mm()), h.state().canvas.viewport);
    for pressed in [true, false] {
        h.event(Event::PointerMoved(p));
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(5);
}
#[test]
fn actual_board_click_highlights_and_navigates_symbol_without_propagating_selection_or_edits() {
    let e = engine();
    let row = e.query_linked_components(&ComponentLinkQuery::default()).unwrap().components.remove(0);
    let mut h = harness(&e);
    select_board(&mut h, row.board_position.unwrap());
    assert_eq!(e.selection().ids, vec![row.part_id.unwrap()]);
    let before = e.session_state();
    let dirty = e.document_status();
    settle(&mut h, "Show schematic counterpart");
    assert_eq!(h.state().cross_probe.highlighted_symbols(), vec![row.symbol_id.unwrap()]);
    assert!(!h.state().schematic.active, "selection alone does not switch mode");
    click(&mut h, "Show schematic counterpart");
    assert!(h.state().schematic.active);
    let at = row.schematic_position.unwrap();
    assert_eq!(h.state().schematic.camera.center, WorldPos::new(at.x.to_mm(), at.y.to_mm()));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), dirty);
    // A source clear and identical reselect must query again, even at the same model cursor.
    e.set_selection(SetSelection { ids: vec![], mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    h.run_steps(5);
    assert!(h.state().cross_probe.highlighted_symbols().is_empty());
    e.set_selection(SetSelection { ids: vec![row.part_id.unwrap()], mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    settle(&mut h, "Show schematic counterpart");
    assert_eq!(h.state().cross_probe.highlighted_symbols(), vec![row.symbol_id.unwrap()]);
}
#[test]
fn actual_schematic_select_highlights_hidden_pcb_counterpart_and_navigates_without_shared_selection_changes() {
    let e = engine();
    let row = e.query_linked_components(&ComponentLinkQuery::default()).unwrap().components.remove(0);
    let mut h = harness(&e);
    click(&mut h, "Schematic");
    click(&mut h, "Schematic objects");
    click(&mut h, &format!("Symbol {}", row.symbol_reference.as_ref().unwrap()));
    settle(&mut h, "Show PCB counterpart");
    assert_eq!(h.state().cross_probe.highlighted_parts(), vec![row.part_id.unwrap()]);
    assert!(e.selection().ids.is_empty());
    let before = e.session_state();
    for layer in [LayerId::F_CU, LayerId::F_MASK, LayerId::F_SILKS, LayerId::B_CU, LayerId::B_MASK, LayerId::B_SILKS] {
        h.state_mut().layers.set_visible(layer, false);
    }
    h.run_steps(5);
    assert!(h.query_by_label("PCB counterpart is on hidden layers; visibility is preserved.").is_some());
    let layers = h.state().layers.clone();
    click(&mut h, "Show PCB counterpart");
    assert!(!h.state().schematic.active);
    let at = row.board_position.unwrap();
    assert_eq!(h.state().canvas.camera.center, WorldPos::new(at.x.to_mm(), at.y.to_mm()));
    assert_eq!(h.state().layers, layers);
    assert_eq!(e.session_state(), before);
    // Native canvas resize must preserve explicit navigation rather than startup auto-fit.
    h.run_steps(5);
    assert_eq!(h.state().canvas.camera.center, WorldPos::new(at.x.to_mm(), at.y.to_mm()));
}
#[test]
fn peer_unlink_and_same_id_reopen_clear_counterparts_and_stale_navigation() {
    let e = engine();
    let row = e.query_linked_components(&ComponentLinkQuery::default()).unwrap().components.remove(0);
    let mut h = harness(&e);
    select_board(&mut h, row.board_position.unwrap());
    settle(&mut h, "Show schematic counterpart");
    let before = e.session_state();
    let part = before.project.board.parts[&row.part_id.unwrap()].clone();
    e.execute(
        "link_schematic_symbol",
        serde_json::json!({"expected_project":before.project.id,"expected_part":part,"target":{"kind":"unlink"}}),
        Origin::agent("peer"),
    )
    .unwrap();
    settle(&mut h, "Component is unlinked; establish a stable link through synchronization.");
    assert!(h.state().cross_probe.highlighted_symbols().is_empty());
    assert!(h.query_by_label("Show schematic counterpart").is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    let old = e.event_cursor();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: Some(old) }, Origin::User).unwrap();
    h.run_steps(5);
    assert!(h.state().cross_probe.highlighted_symbols().is_empty());
    assert!(h.query_by_label("Show schematic counterpart").is_none());
    assert_ne!(e.event_cursor().stream_id, old.stream_id);
}
#[test]
fn unsupported_selection_and_large_selection_have_explicit_messages() {
    let e = engine();
    let track = e
        .execute("add_track", serde_json::json!({"start_x_mm":0,"start_y_mm":0,"end_x_mm":3,"end_y_mm":0,"width_mm":0.2,"layer":"top"}), Origin::User)
        .unwrap()
        .created[0];
    let mut h = harness(&e);
    e.set_selection(SetSelection { ids: vec![track], mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    settle(&mut h, "1 selected objects are not components.");
    assert!(h.state().cross_probe.highlighted_symbols().is_empty());
    let mut ids: Vec<ObjectId> = vec![track];
    for n in 0..100 {
        ids.push(
            e.execute(
                "add_part",
                serde_json::json!({"designator":format!("EXTRA{n}"),"value":"","footprint":"preview","x_mm":n,"y_mm":0}),
                Origin::User,
            )
            .unwrap()
            .created[0],
        );
    }
    e.set_selection(SetSelection { ids, mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    settle(&mut h, "Cross-probing supports at most 100 selected objects; reduce the selection.");
    assert!(h.state().cross_probe.highlighted_symbols().is_empty());
}

#[test]
fn active_route_preview_is_preserved_when_counterpart_navigation_is_rejected() {
    let mut project = engine().read(Clone::clone);
    project.board.outline = Some(
        viamind_doc::BoardOutline::new(
            viamind_geom::Polygon::new(vec![
                viamind_geom::Point::from_mm(0.0, 0.0),
                viamind_geom::Point::from_mm(100.0, 0.0),
                viamind_geom::Point::from_mm(100.0, 100.0),
                viamind_geom::Point::from_mm(0.0, 100.0),
            ]),
            vec![],
        )
        .unwrap(),
    );
    let e = EngineHandle::new(project);
    let row = e.query_linked_components(&ComponentLinkQuery::default()).unwrap().components.remove(0);
    let mut h = harness(&e);
    select_board(&mut h, row.board_position.unwrap());
    settle(&mut h, "Show schematic counterpart");
    let before = e.session_state();
    h.state_mut().begin_route();
    h.run_steps(5);
    click(&mut h, "Start route");
    select_board(&mut h, viamind_geom::Point::from_mm(70.0, 70.0));
    assert!(h.state().routing.previewing());
    let points = h.state().routing.points().to_vec();
    assert_eq!(points.len(), 1);
    click(&mut h, "Show schematic counterpart");
    assert!(!h.state().schematic.active);
    assert!(h.query_by_label("Finish the active edit before navigating to a counterpart.").is_some());
    assert!(h.state().routing.previewing());
    assert_eq!(h.state().routing.points(), points);
    assert_eq!(e.session_state(), before);
}
