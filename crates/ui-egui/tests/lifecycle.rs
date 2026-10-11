//! Exercise real forms, confirmation choices and native close interception.
use egui::{Event, Key, Modifiers, ViewportCommand, ViewportEvent, ViewportId, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{Project, file};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;

fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    h
}
fn shortcut(h: &mut Harness<'_, ViamindApp>, key: Key) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(5);
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).focus();
    shortcut(h, Key::A);
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
}
fn new_form(h: &mut Harness<'_, ViamindApp>, name: &str) {
    shortcut(h, Key::N);
    input(h, "Project name", name);
    click(h, "Create project");
}
fn close_command(h: &Harness<'_, ViamindApp>) -> bool {
    h.output().viewport_output[&ViewportId::ROOT].commands.iter().any(|command| matches!(command, ViewportCommand::Close))
}

#[test]
fn new_cancel_stale_discard_and_open_share_the_live_engine() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("other project.viamind.json");
    let other = Project::new("Other");
    file::save_new(&other, &path).unwrap();
    let engine = EngineHandle::new(Project::new("Original"));
    let mut h = harness(&engine);
    h.state_mut().place_test_resistor();
    new_form(&mut h, "Canceled");
    h.get_by_label("Save changes before continuing?");
    click(&mut h, "Cancel");
    assert_eq!(engine.read(|p| p.name.clone()), "Original");
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);
    new_form(&mut h, "New board");
    // Agent edits after confirmation appeared: the first discard must reject.
    h.state_mut().place_test_resistor();
    click(&mut h, "Discard and continue");
    assert_eq!(engine.read(|p| p.board.parts.len()), 2);
    h.get_by_label("Save changes before continuing?");
    click(&mut h, "Discard and continue");
    assert_eq!(engine.read(|p| p.name.clone()), "New board");
    assert!(engine.read(|p| p.board.parts.is_empty()));
    assert!(!engine.can_undo());
    assert_eq!(h.state().scene.items.len(), 4, "only the board outline remains");
    shortcut(&mut h, Key::O);
    input(&mut h, "Project file", path.to_str().unwrap());
    click(&mut h, "Open file");
    assert_eq!(engine.read(Clone::clone), other);
    assert_eq!(engine.document_status().path, Some(path));
}

#[test]
fn save_before_new_and_cancel_save_as_preserve_the_current_design() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("before new.viamind.json");
    let engine = EngineHandle::new(Project::new("Keep on disk"));
    let mut h = harness(&engine);
    h.state_mut().place_test_resistor();
    new_form(&mut h, "Replacement");
    click(&mut h, "Save changes");
    click(&mut h, "Cancel");
    assert_eq!(engine.read(|p| p.name.clone()), "Keep on disk");
    assert!(engine.document_status().dirty);
    new_form(&mut h, "Replacement");
    click(&mut h, "Save changes");
    input(&mut h, "Project file", path.to_str().unwrap());
    click(&mut h, "Save file");
    assert_eq!(file::load(&path).unwrap().name, "Keep on disk");
    assert_eq!(file::load(&path).unwrap().board.parts.len(), 1);
    assert_eq!(engine.read(|p| p.name.clone()), "Replacement");
    assert!(!engine.document_status().dirty);
    assert!(engine.document_status().path.is_none());
}

#[test]
fn native_close_is_canceled_and_requires_a_current_discard_confirmation() {
    let engine = EngineHandle::new(Project::new("Close guard"));
    let mut h = harness(&engine);
    h.state_mut().place_test_resistor();
    h.input_mut().viewports.get_mut(&ViewportId::ROOT).unwrap().events.push(ViewportEvent::Close);
    h.run_steps(1);
    assert!(h.output().viewport_output[&ViewportId::ROOT].commands.iter().any(|command| matches!(command, ViewportCommand::CancelClose)));
    h.input_mut().viewports.get_mut(&ViewportId::ROOT).unwrap().events.clear();
    h.run_steps(1);
    click(&mut h, "Cancel");
    assert!(!close_command(&h));
    h.input_mut().viewports.get_mut(&ViewportId::ROOT).unwrap().events.push(ViewportEvent::Close);
    h.run_steps(1);
    h.input_mut().viewports.get_mut(&ViewportId::ROOT).unwrap().events.clear();
    h.run_steps(1);
    h.get_by_label("Discard and continue").click();
    h.run_steps(1);
    assert!(close_command(&h));
}

#[test]
fn failed_save_and_edits_arriving_after_save_keep_the_document_open() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let directory = tempfile::tempdir().unwrap();
    let engine = EngineHandle::new(Project::new("Concurrent"));
    let mut h = harness(&engine);
    h.state_mut().place_test_resistor();
    new_form(&mut h, "Must wait");
    click(&mut h, "Save changes");
    let bad_path = directory.path().join("missing/board.json");
    input(&mut h, "Project file", bad_path.to_str().unwrap());
    click(&mut h, "Save file");
    h.get_by_label("Save project as");
    assert_eq!(engine.read(|p| p.name.clone()), "Concurrent");
    assert!(engine.document_status().dirty);
    let path = directory.path().join("snapshot.json");
    input(&mut h, "Project file", path.to_str().unwrap());
    let callback = engine.clone();
    let edited = Arc::new(AtomicBool::new(false));
    let flag = edited.clone();
    engine.on_change(move || {
        if !flag.swap(true, Ordering::SeqCst) {
            callback
                .execute(
                    "add_part",
                    serde_json::json!({ "designator": "Agent", "value": "", "footprint": "F", "x_mm": 0, "y_mm": 0 }),
                    viamind_engine::Origin::agent("late-edit"),
                )
                .unwrap();
        }
    });
    click(&mut h, "Save file");
    assert!(edited.load(Ordering::SeqCst));
    assert_eq!(file::load(path).unwrap().board.parts.len(), 1);
    assert_eq!(engine.read(|p| p.board.parts.len()), 2);
    assert_eq!(engine.read(|p| p.name.clone()), "Concurrent");
    h.get_by_label("Save changes before continuing?");
    click(&mut h, "Cancel");
    assert!(engine.document_status().dirty);
}
