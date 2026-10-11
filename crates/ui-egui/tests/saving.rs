//! Actual Save As dialog and keyboard entry points use the shared action.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{Project, file};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;

fn shortcut(h: &mut Harness<'_, ViamindApp>, modifiers: Modifiers, key: Key) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.run_steps(2);
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(1);
}

#[test]
fn save_as_dialog_writes_a_native_file_then_ctrl_s_reuses_the_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("UI board with spaces.viamind.json");
    let engine = EngineHandle::new(Project::new("UI save"));
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.place_test_resistor();
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(2);
    shortcut(&mut h, Modifiers::COMMAND, Key::S);
    h.get_by_label("Save project as");
    h.get_by_label("Project file").focus();
    shortcut(&mut h, Modifiers::COMMAND, Key::A);
    h.get_by_label("Project file").type_text(path.to_str().unwrap());
    h.run_steps(2);
    h.get_by_label("Save file").click();
    h.run_steps(2);
    assert_eq!(file::load(&path).unwrap(), engine.read(Clone::clone));
    assert!(!engine.document_status().dirty);
    h.state_mut().place_test_resistor();
    assert!(engine.document_status().dirty);
    shortcut(&mut h, Modifiers::COMMAND, Key::S);
    assert_eq!(file::load(&path).unwrap().board.parts.len(), 2);
    assert!(!engine.document_status().dirty);
    shortcut(&mut h, Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
    h.get_by_label("Save project as");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert_eq!(file::load(&path).unwrap().board.parts.len(), 2);
}
