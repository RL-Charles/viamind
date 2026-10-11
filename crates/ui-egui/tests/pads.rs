//! Actual pad form applies shared commands and keeps stale edits open.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
}
#[test]
fn desktop_pad_form_checks_sizes_assigns_nets_and_rejects_stale_edits() {
    let engine = EngineHandle::new(Project::new("Pad UI"));
    let id =
        engine.execute("add_part", json!({"designator":"R1","value":"10k","footprint":"R_0603","x_mm":10,"y_mm":20}), Origin::User).unwrap().created
            [0];
    let part = engine.read(|p| p.board.part(&id).unwrap().clone());
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_edit_pads(part);
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    input(&mut h, "Width (mm) 1", "-1");
    click(&mut h, "Apply pads");
    assert!(engine.read(|p| p.board.part(&id).unwrap().pads.is_none()));
    input(&mut h, "Width (mm) 1", "0.9");
    input(&mut h, "Net 1", "GND");
    click(&mut h, "Apply pads");
    let part = engine.read(|p| p.board.part(&id).unwrap().clone());
    assert_eq!(part.pads.as_ref().unwrap().0[0].net.as_deref(), Some("GND"));
    assert!(h.state().scene.items.iter().any(|i| matches!(i.prim, viamind_ui_egui::scene::Prim::Quad(_))));
    h.state_mut().begin_edit_pads(part.clone());
    h.run_steps(5);
    engine.execute("move_part", json!({"id":id,"x_mm":30,"y_mm":20}), Origin::agent("mover")).unwrap();
    input(&mut h, "Net 1", "VCC");
    click(&mut h, "Apply pads");
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().pads.as_ref().unwrap().0[0].net.clone()), Some("GND".into()));
    click(&mut h, "Reload pads");
    input(&mut h, "Net 1", "VCC");
    click(&mut h, "Apply pads");
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().pads.as_ref().unwrap().0[0].id), part.pads.as_ref().unwrap().0[0].id);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().pads.as_ref().unwrap().0[0].net.clone()), Some("GND".into()));
}
