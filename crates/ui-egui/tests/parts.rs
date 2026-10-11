//! Normal placement and selected-part property workflows use shared actions.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_doc::{Project, Side};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;

fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 900.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).focus();
    h.run_steps(2);
    assert!(h.get_by_label(label).is_focused());
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
    assert_eq!(h.get_by_label(label).accesskit_node().value().as_deref(), Some(value));
}
#[test]
fn place_edit_delete_and_undo_use_the_same_engine() {
    let engine = EngineHandle::new(Project::new("Desktop editing"));
    let mut h = harness(&engine);
    click(&mut h, "Place part…");
    input(&mut h, "Designator", "C7");
    input(&mut h, "Value", "100nF");
    input(&mut h, "Footprint", "C_0805");
    input(&mut h, "X (mm)", "-1.234567");
    input(&mut h, "Rotation (deg)", "90");
    click(&mut h, "Bottom");
    click(&mut h, "Place on board");
    let part = engine.read(|p| p.board.part_by_designator("C7").unwrap().clone());
    assert_eq!(part.position.x.0, -1_234_567);
    assert_eq!(part.rotation_mdeg, 90_000);
    assert_eq!(part.side, Side::Bottom);
    assert_eq!(engine.selection().ids, [part.id]);
    input(&mut h, "Value", "220nF");
    input(&mut h, "Y (mm)", "2.000001");
    click(&mut h, "Apply properties");
    let changed = engine.read(|p| p.board.part(&part.id).unwrap().clone());
    assert_eq!(changed.value, "220nF");
    assert_eq!(changed.position.y.0, 2_000_001);
    assert_eq!(engine.with_engine(|e| e.undo_history().len()), 2);
    click(&mut h, "Delete part");
    assert!(engine.read(|p| p.board.parts.is_empty()));
    assert!(engine.selection().ids.is_empty());
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(engine.read(|p| p.board.part(&part.id).unwrap().clone()), changed);
}
#[test]
fn stale_properties_require_reload_and_invalid_placement_keeps_the_form() {
    let engine = EngineHandle::new(Project::new("Stale editing"));
    let mut h = harness(&engine);
    click(&mut h, "Place part…");
    input(&mut h, "X (mm)", "not a number");
    click(&mut h, "Place on board");
    assert!(engine.read(|p| p.board.parts.is_empty()));
    click(&mut h, "Cancel");
    assert!(engine.read(|p| p.board.parts.is_empty()));
    click(&mut h, "Place part…");
    click(&mut h, "Place on board");
    let part = engine.read(|p| p.board.parts.values().next().unwrap().clone());
    input(&mut h, "Value", "User draft");
    engine.execute("update_part", json!({ "id": part.id, "value": "Agent value" }), Origin::agent("property-writer")).unwrap();
    h.run_steps(5);
    click(&mut h, "Apply properties");
    assert_eq!(engine.read(|p| p.board.part(&part.id).unwrap().value.clone()), "Agent value");
    h.get_by_label("Part changed; reload to review the latest properties.");
    click(&mut h, "Reload");
    input(&mut h, "Designator", "R9");
    click(&mut h, "Apply properties");
    assert_eq!(engine.read(|p| p.board.part(&part.id).unwrap().designator.clone()), "R9");
    assert_eq!(engine.read(|p| p.board.part(&part.id).unwrap().value.clone()), "Agent value");
}
#[test]
fn placement_opened_before_new_document_cannot_modify_the_new_document() {
    let engine = EngineHandle::new(Project::new("Original"));
    let mut h = harness(&engine);
    click(&mut h, "Place part…");
    engine.new_project(Default::default(), Origin::agent("document-writer")).unwrap();
    h.run_steps(5);
    click(&mut h, "Place on board");
    assert!(engine.read(|p| p.board.parts.is_empty()));
    h.get_by_label("Place part");
    click(&mut h, "Cancel");
}
