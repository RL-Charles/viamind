//! Actual accessible events; conversion remains separate from applying pads.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_engine::EngineHandle;
use viamind_ui_egui::{ViamindApp, actions::UiAction};
const SOURCE: &str = include_str!("../../../fixtures/import/original-cardinal-pads.kicad_mod");
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
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
    while h.state().footprint_conversion.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().footprint_conversion.busy());
}
#[test]
fn actual_menu_and_dirty_session_conversion_leave_project_and_drafts_unchanged() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_mod");
    let output = directory.path().join("new.json");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap());
    UiAction::PlacePart.run(&engine, json!({"designator":"KEEP1","value":"dirty","footprint":"old","x_mm":0,"y_mm":0})).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    assert!(status.dirty);
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "Draw wires on canvas");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !h.state().schematic_wires.previewing() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.state().schematic_wires.active());
    click(&mut h, "File");
    click(&mut h, "Convert footprint S-expression…");
    input(&mut h, "Footprint source file", source.to_str().unwrap());
    input(&mut h, "New native footprint file", output.to_str().unwrap());
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    assert!(h.state().footprint_conversion.preview().unwrap().can_convert);
    let preview = h.state().footprint_conversion.preview().unwrap();
    let pad = &preview.footprint.as_ref().unwrap().pads[0];
    assert_eq!((pad.width.0, pad.height.0), (1_200_003, 800_001));
    assert!(preview.report.items.iter().any(|i| i.reason.contains("source angle 90 degrees, canonical 90 degrees")));
    assert!(h.query_all_by_label_contains("2 rectangular pads").next().is_some());
    click(&mut h, "Create new native footprint file");
    wait(&mut h);
    assert_eq!(viamind_doc::footprint::load(&output).unwrap().pads.len(), 2);
    assert!(h.state().schematic_wires.active(), "conversion must retain the live wire draft");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    assert!(h.query_all_by_label_contains("Completed source snapshot report").next().is_some());
}
#[test]
fn actual_compact_scroll_reaches_conversion_and_changed_source_error_without_touching_work() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_mod");
    let output = directory.path().join("new.json");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_conversion();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "Footprint source file", source.to_str().unwrap());
    input(&mut h, "New native footprint file", output.to_str().unwrap());
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    h.get_by_label("Create new native footprint file").scroll_to_me();
    h.run_steps(8);
    let rect = h.get_by_label("Create new native footprint file").rect();
    assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= 640.0 && rect.max.y <= 400.0, "{rect:?}");
    std::fs::write(&source, format!("{SOURCE}\n")).unwrap();
    click(&mut h, "Create new native footprint file");
    wait(&mut h);
    let label = h.query_all_by_label_contains("source bytes changed").next().unwrap();
    label.scroll_to_me();
    h.run_steps(8);
    let rect = h.query_all_by_label_contains("source bytes changed").next().unwrap().rect();
    assert!(rect.min.y >= 0.0 && rect.max.y <= 400.0, "{rect:?}");
    assert!(!output.exists());
    assert_eq!(engine.session_state(), before);
}
#[test]
fn actual_report_scroll_reaches_exact_cardinal_normalization_without_session_changes() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("cardinal.kicad_mod");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_conversion();
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "Footprint source file", source.to_str().unwrap());
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    click(&mut h, "Footprint per-list fidelity report");
    let label = "source angle 90 degrees, canonical 90 degrees";
    let header = h.get_by_label("Footprint per-list fidelity report").rect();
    let inside_report = egui::pos2(header.center().x, header.max.y + 120.0);
    assert!(inside_report.y < 1000.0);
    for _ in 0..150 {
        if h.query_by_label_contains(label).is_some() {
            break;
        }
        // Anchor input inside the report rather than on its clipped virtual rows.
        h.event(Event::PointerMoved(inside_report));
        h.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -100.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(8);
    }
    h.get_by_label_contains(label).scroll_to_me();
    h.run_steps(8);
    let rect = h.get_by_label_contains(label).rect();
    assert!(rect.min.y >= 0.0 && rect.max.y <= 1000.0, "{rect:?}");
    assert!(h.query_by_label_contains("center unchanged; width and height swapped exactly").is_some());
    assert_eq!(engine.session_state(), before);
}
#[test]
fn actual_context_and_path_changes_discard_preview_and_unsupported_source_never_enables_conversion() {
    let _serial = SERIAL.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_mod");
    let output = directory.path().join("new.json");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_conversion();
    let mut h = Harness::builder().with_size(vec2(1000.0, 900.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "Footprint source file", source.to_str().unwrap());
    input(&mut h, "New native footprint file", output.to_str().unwrap());
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    UiAction::PlacePart.run(&engine, json!({"designator":"PEER1","value":"keep","footprint":"old","x_mm":0,"y_mm":0})).unwrap();
    h.run_steps(3);
    assert!(h.state().footprint_conversion.preview().is_none());
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    input(&mut h, "New native footprint file", directory.path().join("other.json").to_str().unwrap());
    assert!(h.state().footprint_conversion.preview().is_none());
    std::fs::write(&source, SOURCE.replace("smd rect", "smd circle")).unwrap();
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    assert!(!h.state().footprint_conversion.preview().unwrap().can_convert);
    click(&mut h, "Create new native footprint file");
    assert!(!output.exists());
    assert!(!directory.path().join("other.json").exists());
}
