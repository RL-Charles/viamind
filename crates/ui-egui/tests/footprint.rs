//! Drive the real library controls, including stale draft and destination failures.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::{Project, footprint};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;

fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn path(h: &mut Harness<'_, ViamindApp>, value: &str) {
    input(h, "Library file path", value);
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
fn library_form_exports_committed_pads_reuses_geometry_and_rejects_stale_parts() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("shared.footprint.json");
    let engine = EngineHandle::new(Project::new("Library UI"));
    let add = |name| {
        engine.execute("add_part", json!({"designator":name,"value":"10k","footprint":"Reusable","x_mm":10,"y_mm":20}), Origin::User).unwrap().created
            [0]
    };
    let source = add("R1");
    let target = add("R2");
    let source_part = engine.read(|p| p.board.part(&source).unwrap().clone());
    engine.execute("set_part_pads",json!({"id":source,"expected":source_part,"pads":[{"number":"1","x_mm":-0.800001,"y_mm":0,"width_mm":0.900003,"height_mm":0.95,"net":"SOURCE_NET"}]}),Origin::User).unwrap();
    let source_part = engine.read(|p| p.board.part(&source).unwrap().clone());
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_edit_pads(source_part.clone());
    let mut h = Harness::builder().with_size(vec2(1600., 1000.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    path(&mut h, library.to_str().unwrap());
    input(&mut h, "Width (mm) 1", "8.0");
    input(&mut h, "Net 1", "UNAPPLIED_DRAFT");
    click(&mut h, "Export applied footprint");
    let f = footprint::load(&library).unwrap();
    assert_eq!(f.pads[0].position.x.0, -800001);
    assert_eq!(f.pads[0].width.0, 900003);
    let bytes = std::fs::read(&library).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains("SOURCE_NET") && !text.contains("UNAPPLIED_DRAFT") && !text.contains("\"id\""));
    click(&mut h, "Export applied footprint");
    assert_eq!(std::fs::read(&library).unwrap(), bytes);
    click(&mut h, "Cancel");
    let target_part = engine.read(|p| p.board.part(&target).unwrap().clone());
    h.state_mut().begin_edit_pads(target_part.clone());
    h.run_steps(5);
    path(&mut h, library.to_str().unwrap());
    engine.execute("move_part", json!({"id":target,"x_mm":30,"y_mm":20}), Origin::agent("mover")).unwrap();
    let state = engine.session_state();
    click(&mut h, "Replace pads from library");
    assert_eq!(engine.session_state(), state);
    click(&mut h, "Reload pads");
    click(&mut h, "Replace pads from library");
    let applied = engine.read(|p| p.board.part(&target).unwrap().clone());
    assert_eq!(applied.position.x.to_mm(), 30.);
    let pad = &applied.pads.as_ref().unwrap().0[0];
    assert!(pad.net.is_none());
    assert_ne!(pad.id, source_part.pads.as_ref().unwrap().0[0].id);
    assert_eq!(pad.position.x.0, -800001);
    click(&mut h, "Cancel");
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(engine.session_state().project, state.project);
}
