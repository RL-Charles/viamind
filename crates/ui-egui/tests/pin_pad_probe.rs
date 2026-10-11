//! Actual desktop terminal row and explicit navigation inputs.
use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::file;
use viamind_engine::{EngineHandle, Origin, PinPadLinkQuery, SelectionMode, SetSelection};
use viamind_ui_egui::{ViamindApp, camera::WorldPos, layers::LayerId};
fn setup() -> (EngineHandle, Harness<'static, ViamindApp>, viamind_engine::PinPadLinkResult) {
    let e = EngineHandle::new(file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
    let id = e.read(|p| *p.board.parts.keys().next().unwrap());
    e.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Replace }, Origin::agent("fixture")).unwrap();
    let report = e.query_linked_pin_pads(&PinPadLinkQuery { component_id: id, offset: 0, limit: None }).unwrap();
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 2000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    settle(&mut h, "Inspect pins and pads");
    click(&mut h, "Inspect pins and pads");
    settle(&mut h, "Show schematic pin");
    (e, h, report)
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
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn caption(row: &viamind_engine::LinkedPinPad) -> String {
    format!("{} {} · pin present / pad present", row.number, row.pin_name.as_deref().unwrap_or(""))
}
#[test]
fn actual_terminal_choice_and_bidirectional_navigation_preserve_entire_session_and_component_selection() {
    let (e, mut h, r) = setup();
    let before = e.session_state();
    let status = e.document_status();
    let row = &r.terminals[0];
    assert!(h.state().pin_pad_probe.selected_terminal().is_none());
    click(&mut h, &caption(row));
    assert_eq!(h.state().pin_pad_probe.selected_terminal(), Some((row.pin_id.unwrap(), row.pad_id.unwrap())));
    click(&mut h, "Show schematic pin");
    assert!(h.state().schematic.active);
    let at = row.schematic_position.unwrap();
    assert_eq!(h.state().schematic.camera.center, WorldPos::new(at.x.to_mm(), at.y.to_mm()));
    click(&mut h, "Show PCB pad");
    assert!(!h.state().schematic.active);
    let at = row.board_position.unwrap();
    assert_eq!(h.state().canvas.camera.center, WorldPos::new(at.x.to_mm(), at.y.to_mm()));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
#[test]
fn hidden_copper_and_active_edit_rejections_preserve_local_preview_and_session() {
    let (e, mut h, r) = setup();
    click(&mut h, &caption(&r.terminals[0]));
    let before = e.session_state();
    h.state_mut().layers.set_visible(LayerId::F_CU, false);
    h.state_mut().layers.set_visible(LayerId::B_CU, false);
    let layers = h.state().layers.clone();
    let camera = h.state().canvas.camera;
    click(&mut h, "Show PCB pad");
    assert!(h.query_by_label("Destination copper layer is hidden; enable it explicitly before navigating.").is_some());
    assert_eq!(h.state().layers, layers);
    assert_eq!(h.state().canvas.camera, camera);
    click(&mut h, "Show schematic pin");
    click(&mut h, "Draw wires on canvas");
    settle(&mut h, "0 confirmed vertices");
    click(&mut h, "Show PCB pad");
    assert!(h.query_by_label("Finish or cancel the active edit before terminal navigation.").is_some());
    assert!(h.state().schematic_wires.active());
    assert_eq!(e.session_state(), before);
}
#[test]
fn peer_edit_selection_replacement_and_same_uuid_reopen_clear_terminal_highlights() {
    let (e, mut h, r) = setup();
    click(&mut h, &caption(&r.terminals[0]));
    e.execute("update_part", serde_json::json!({"id":r.component.part_id,"value":"peer"}), Origin::agent("peer")).unwrap();
    h.run_steps(5);
    assert!(h.state().pin_pad_probe.selected_terminal().is_none());
    settle(&mut h, "Inspect pins and pads");
    click(&mut h, "Inspect pins and pads");
    settle(&mut h, "Show schematic pin");
    click(&mut h, &caption(&r.terminals[0]));
    e.set_selection(SetSelection { ids: vec![], mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    h.run_steps(5);
    assert!(h.state().pin_pad_probe.selected_terminal().is_none());
    e.set_selection(SetSelection { ids: vec![r.component.part_id.unwrap()], mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    settle(&mut h, "Inspect pins and pads");
    click(&mut h, "Inspect pins and pads");
    settle(&mut h, "Show schematic pin");
    click(&mut h, &caption(&r.terminals[0]));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    let cursor = e.event_cursor();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: Some(cursor) }, Origin::User).unwrap();
    h.run_steps(5);
    assert_ne!(e.event_cursor().stream_id, cursor.stream_id);
    assert!(h.state().pin_pad_probe.selected_terminal().is_none());
    assert!(h.query_by_label("Show schematic pin").is_none());
}

#[test]
fn actual_schematic_source_keeps_local_component_selection_and_empty_shared_selection() {
    let (e, mut h, r) = setup();
    e.set_selection(SetSelection { ids: vec![], mode: SelectionMode::Replace }, Origin::agent("peer")).unwrap();
    click(&mut h, "Schematic");
    click(&mut h, "Schematic objects");
    click(&mut h, &format!("Symbol {}", r.component.symbol_reference.as_ref().unwrap()));
    settle(&mut h, "Inspect pins and pads");
    click(&mut h, "Inspect pins and pads");
    settle(&mut h, "Show PCB pad");
    let before = e.session_state();
    click(&mut h, &caption(&r.terminals[0]));
    click(&mut h, "Show PCB pad");
    assert!(!h.state().schematic.active);
    click(&mut h, "Show schematic pin");
    assert!(h.state().schematic.active);
    assert_eq!(h.state().cross_probe.highlighted_parts(), vec![r.component.part_id.unwrap()]);
    assert_eq!(e.session_state(), before);
    assert!(e.selection().ids.is_empty());
}
