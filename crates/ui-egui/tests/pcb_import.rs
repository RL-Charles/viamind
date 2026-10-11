//! Real accessible desktop events for guarded standalone PCB import.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::EngineHandle;
use viamind_ui_egui::{ViamindApp, actions::UiAction};
const SOURCE: &str = include_str!("../../../fixtures/import/original-two-layer.kicad_pcb");
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().pcb_import.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().pcb_import.busy());
}
#[test]
fn actual_file_menu_preview_and_apply_create_unsaved_native_geometry() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.kicad_pcb");
    std::fs::write(&path, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("keep"));
    let before = engine.session_state();
    let status = engine.document_status();
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "File");
    click(&mut h, "Import PCB S-expression…");
    input(&mut h, "PCB source file", path.to_str().unwrap());
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    assert!(h.state().pcb_import.preview().unwrap().can_import);
    assert!(h.query_all_by_label_contains("omitted lists").next().is_some());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    click(&mut h, "Apply PCB import as unsaved native document");
    wait(&mut h);
    assert_ne!(engine.event_cursor().stream_id, before.cursor.stream_id);
    assert!(engine.document_status().dirty);
    assert!(engine.document_status().path.is_none());
    assert_eq!(engine.read(|p| p.board.parts.len()), 1);
    assert_eq!(engine.read(|p| p.board.tracks.len()), 2);
    assert_eq!(std::fs::read_to_string(path).unwrap(), SOURCE);
    assert!(!engine.can_undo());
    assert!(!engine.can_redo());
    assert!(h.query_by_label("Completed conversion report for the imported source snapshot; later native edits are not included.").is_some());
}
#[test]
fn actual_source_change_failure_and_peer_edit_staleness_preserve_native_work() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.kicad_pcb");
    std::fs::write(&path, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("keep"));
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_pcb_import();
    let mut h = Harness::builder().with_size(vec2(1000.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "PCB source file", path.to_str().unwrap());
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    std::fs::write(&path, format!("{SOURCE}\n")).unwrap();
    click(&mut h, "Apply PCB import as unsaved native document");
    wait(&mut h);
    assert_eq!(engine.session_state(), before);
    assert!(h.query_all_by_label_contains("source bytes changed").next().is_some());
    std::fs::write(&path, SOURCE).unwrap();
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    UiAction::PlacePart.run(&engine, json!({"designator":"KEEP1","value":"unsaved","footprint":"test","x_mm":0,"y_mm":0})).unwrap();
    h.run_steps(3);
    assert!(h.state().pcb_import.preview().is_none());
    assert!(h.query_by_label("Save current native changes before importing. Unsaved work is protected.").is_some());
    let dirty = engine.session_state();
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    click(&mut h, "Apply PCB import as unsaved native document");
    assert_eq!(engine.session_state(), dirty);
}
#[test]
fn unsupported_report_and_local_draft_guards_are_visible_without_cancellation() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blocked.kicad_pcb");
    std::fs::write(&path, SOURCE.replace("smd rect", "smd roundrect")).unwrap();
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);

    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Draw wires on canvas");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !h.state().schematic_wires.previewing() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.state().schematic_wires.active());
    click(&mut h, "File");
    click(&mut h, "Import PCB S-expression…");
    input(&mut h, "PCB source file", path.to_str().unwrap());
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    assert!(!h.state().pcb_import.preview().unwrap().can_import);
    assert!(h.query_by_label("Unsupported or invalid geometry blocks Apply. Repair or export the supported subset.").is_some());
    assert!(h.query_by_label("Finish or cancel active editing drafts before importing; drafts are preserved.").is_some());
    click(&mut h, "Apply PCB import as unsaved native document");
    assert!(h.state().schematic_wires.active());
    assert_eq!(engine.session_state(), before);
}

#[test]
fn compact_actual_scroll_reaches_apply_and_source_error_within_viewport() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.kicad_pcb");
    std::fs::write(&path, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("keep"));
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_pcb_import();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    input(&mut h, "PCB source file", path.to_str().unwrap());
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    assert!(h.state().pcb_import.preview().unwrap().can_import);
    let apply = "Apply PCB import as unsaved native document";
    h.get_by_label(apply).scroll_to_me();
    h.run_steps(8);
    let rect = h.get_by_label(apply).rect();
    assert!(rect.min.x >= 0.0 && rect.max.x <= 640.0 && rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    std::fs::write(&path, format!("{SOURCE}\n")).unwrap();
    click(&mut h, apply);
    wait(&mut h);
    h.query_all_by_label_contains("source bytes changed").next().expect("source error").scroll_to_me();
    h.run_steps(8);
    let rect = h.query_all_by_label_contains("source bytes changed").next().expect("visible source error").rect();
    assert!(rect.min.x >= 0.0 && rect.max.x <= 640.0 && rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    assert_eq!(engine.session_state(), before);
    assert!(!engine.document_status().dirty);
}
