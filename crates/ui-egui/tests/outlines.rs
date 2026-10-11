//! Actual polygon form, invalid/stale drafts, and undoable removal.
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
fn outline_form_validates_applies_and_removes_geometry_through_shared_actions() {
    let engine = EngineHandle::new(Project::new("Outline UI"));
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 900.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    input(&mut h, "Outer vertices", "0,0\n40,0\n40,30\n0,30");
    input(&mut h, "Cutout vertices", "0,10\n10,10\n10,20\n0,20");
    click(&mut h, "Apply outline");
    assert!(engine.read(|p| p.board.outline.is_none()));
    input(&mut h, "Cutout vertices", "10,10\n20,10\n20,20\n10,20");
    click(&mut h, "Apply outline");
    assert_eq!(engine.read(|p| p.board.outline.as_ref().unwrap().cutouts.len()), 1);
    assert!(h.state().scene.substrate.is_some());
    assert_eq!(h.state().scene.board.max.x, 40.0);
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    click(&mut h, "Remove outline");
    assert!(engine.read(|p| p.board.outline.is_none()));
    h.state_mut().undo_design();
    h.run_steps(5);
    assert!(engine.read(|p| p.board.outline.is_some()));
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    engine.execute("set_board_outline", json!({"outline":null}), Origin::agent("outline-writer")).unwrap();
    click(&mut h, "Apply outline");
    assert!(engine.read(|p| p.board.outline.is_none()));
    click(&mut h, "Cancel");
    assert!(engine.read(|p| p.board.outline.is_none()));
}

#[test]
fn desktop_dxf_import_reports_errors_and_uses_the_shared_guarded_command() {
    let engine = EngineHandle::new(Project::new("DXF UI"));
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("outline.dxf");
    h.run_steps(5);
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    if h.query_by_label("Import DXF outline").is_none() {
        click(&mut h, "Import ASCII DXF");
    }
    input(&mut h, "DXF file path", path.to_str().unwrap());
    click(&mut h, "Import DXF outline");
    assert!(engine.read(|p| p.board.outline.is_none()));
    std::fs::write(&path, include_str!("../../doc/tests/fixtures/outline-mm.dxf")).unwrap();
    click(&mut h, "Import DXF outline");
    assert_eq!(engine.read(|p| p.board.outline.as_ref().unwrap().cutouts.len()), 1);
    assert!(h.state().scene.substrate.is_some());
    h.state_mut().undo_design();
    h.run_steps(5);
    assert!(engine.read(|p| p.board.outline.is_none()));
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    if h.query_by_label("Import DXF outline").is_none() {
        click(&mut h, "Import ASCII DXF");
    }
    input(&mut h, "DXF file path", path.to_str().unwrap());
    engine
        .execute(
            "import_board_outline_dxf",
            json!({"text":include_str!("../../doc/tests/fixtures/outline-inch.dxf"),"expected":{"project_id":engine.read(|p|p.id),"outline":null}}),
            Origin::agent("dxf-writer"),
        )
        .unwrap();
    let after = engine.read(Clone::clone);
    click(&mut h, "Import DXF outline");
    assert_eq!(engine.read(Clone::clone), after);
    click(&mut h, "Cancel");
}
