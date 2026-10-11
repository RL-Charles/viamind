use egui::{Event, Key, Modifiers, PointerButton, Shape, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, SchematicContent},
    symbol_package::{self, PackageUnit, SchematicPackage},
};
use viamind_engine::EngineHandle;
use viamind_geom::Nm;
use viamind_render::schematic_labels::schematic_label_layout;
use viamind_ui_egui::{ViamindApp, camera::WorldPos};

fn fixture() -> (Project, Sheet) {
    let template = symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json")).unwrap();
    let mut content = SchematicContent::default();
    let mut package = SchematicPackage { id: ObjectId::new(), template_name: template.name.clone(), units: vec![] };
    for (i, unit) in template.units.iter().enumerate() {
        let mut symbol =
            template.unit_template(unit).instantiate("U1", Coordinate { x: Nm(i64::try_from(i).unwrap() * 25_000_000), y: Nm(0) }, 0, false).unwrap();
        symbol.rotation_quarters = u8::try_from(i % 4).unwrap();
        symbol.mirrored = i % 2 == 1;
        package.units.push(PackageUnit { key: unit.key.clone(), name: unit.name.clone(), role: unit.role, symbol_id: symbol.id });
        content.symbols.insert(symbol.id, symbol);
    }
    content.packages.insert(package.id, package);
    let mut sheet = Sheet::new("Readable five units");
    sheet.content = Some(content);
    let mut project = Project::new("Read-only label layout");
    project.schematic.sheets.insert(sheet.id, sheet.clone());
    (project, sheet)
}
fn text_shapes(shape: &Shape, text: &mut Vec<String>) {
    match shape {
        Shape::Text(s) => text.push(s.galley.job.text.clone()),
        Shape::Vec(shapes) => {
            for s in shapes {
                text_shapes(s, text);
            }
        }
        _ => {}
    }
}
fn rendered(h: &Harness<'_, ViamindApp>) -> Vec<String> {
    let mut text = vec![];
    for shape in &h.output().shapes {
        text_shapes(&shape.shape, &mut text);
    }
    text
}
fn click_at(h: &mut Harness<'_, ViamindApp>, at: egui::Pos2) {
    for pressed in [true, false] {
        h.event(Event::PointerMoved(at));
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}
#[test]
fn actual_fit_wheel_center_pick_and_mode_switch_preserve_shared_session_and_geometry() {
    let (project, sheet) = fixture();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(1800.0, 1200.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Fit schematic").click();
    h.run_steps(5);
    let camera = h.state().schematic.camera;
    let viewport = h.state().schematic.viewport;
    assert!(camera.zoom >= 10.0, "wide fit must retain readable labels: {camera:?}");
    for label in schematic_label_layout(&sheet).unwrap() {
        let b = label.envelope;
        let rect = egui::Rect::from_min_max(
            camera.world_to_screen(WorldPos::new(b.min_x as f64 / 1e6, b.min_y as f64 / 1e6), viewport),
            camera.world_to_screen(WorldPos::new(b.max_x as f64 / 1e6, b.max_y as f64 / 1e6), viewport),
        );
        assert!(viewport.contains_rect(rect), "fit lost label {}", label.id);
    }
    let texts = rendered(&h);
    for key in ["A", "B", "C", "D", "P"] {
        assert!(texts.iter().any(|t| t.starts_with(&format!("U1:{key} ("))), "missing {key}: {texts:?}");
    }
    assert!(!texts.iter().any(|t| t.contains("exceeds supported glyph metrics")));
    let symbol = sheet.content.as_ref().unwrap().symbols.values().find(|s| s.position.x.0 == 0).unwrap();
    click_at(&mut h, camera.world_to_screen(WorldPos::new(0.0, 0.0), viewport));
    assert!(h.query_by_label("Apply schematic object").is_some(), "existing exact center picking opens local editor");
    h.get_by_label("Cancel schematic draft").click();
    h.run_steps(3);
    let center = h.state().schematic.viewport.center();
    h.event(Event::PointerMoved(center));
    h.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 50.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(5);
    assert!(h.state().schematic.camera.zoom > camera.zoom);
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::Home, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    assert_eq!(h.state().schematic.camera, camera);
    h.get_by_label("PCB").click();
    h.run_steps(2);
    h.get_by_label("Schematic").click();
    h.run_steps(3);
    assert_eq!(h.state().schematic.camera, camera);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(before.project.schematic.sheets[&sheet.id].content.as_ref().unwrap().symbols[&symbol.id], *symbol);
}
#[test]
fn actual_low_zoom_notice_replaces_symbol_text_without_moving_the_model() {
    let (project, _) = fixture();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.state_mut().schematic.camera.zoom = 5.0;
    h.run_steps(2);
    let texts = rendered(&h);
    assert!(texts.iter().any(|t| t == "Zoom in to read schematic labels"));
    assert!(!texts.iter().any(|t| t.starts_with("U1:A (")));
    assert_eq!(engine.session_state(), before);
}
