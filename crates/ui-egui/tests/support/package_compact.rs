//! Compact input must hit painted controls, not merely virtualized accessibility nodes.
use egui::{Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;

pub(crate) fn engine() -> EngineHandle {
    let mut project = viamind_doc::Project::new("Original package native proof");
    let sheet = viamind_doc::Sheet::new("Complete units");
    project.schematic.sheets.insert(sheet.id, sheet);
    EngineHandle::new(project)
}
pub(crate) fn wait(h: &mut Harness<'_, ViamindApp>, label: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while h.query_by_label_contains(label).is_none() {
        assert!(std::time::Instant::now() < end, "missing {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
}
pub(crate) fn harness(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_package_library();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Refresh complete package target");
    wait(&mut h, "Refreshed complete package target");
    h
}
pub(crate) fn painted_clip(h: &Harness<'_, ViamindApp>, rect: Rect) -> Rect {
    h.output()
        .shapes
        .iter()
        .filter_map(|shape| {
            let egui::Shape::Rect(background) = &shape.shape else { return None };
            let bounds = background.rect;
            (bounds.contains(rect.center()) && (bounds.width() - rect.width()).abs() <= 4.0 && (bounds.height() - rect.height()).abs() <= 4.0)
                .then_some(shape.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("actual painted control shape")
}
pub(crate) fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) -> Rect {
    // This sends the genuine accessibility ScrollIntoView action. Check actual
    // ancestor-clipped painting afterward rather than trusting allocated nodes.
    eprintln!("REVEAL {label}");
    if label.starts_with("Pin ") || label.starts_with("Remove pin ") {
        // Reveal the parent first; virtual child scroll actions cannot reveal
        // an ancestor region while it is entirely below the window clip.
        h.query_by_label_contains("/ 1024 ordered pins").unwrap().scroll_to_me();
        h.run_steps(8);
        let at = h.query_by_label_contains("/ 1024 ordered pins").unwrap().rect().center();
        wheel(h, at, -160.0);
    }
    h.get_by_label(label).scroll_to_me();
    h.run_steps(8);
    let rect = h.get_by_label(label).rect();
    h.hover_at(rect.center());
    h.run_steps(2);
    let rect = h.get_by_label(label).rect();
    eprintln!("RECT {label} {rect:?}");
    let clip = painted_clip(h, rect);
    assert!(clip.contains_rect(rect), "{label}: widget {rect:?}, effective paint clip {clip:?}");
    rect
}
pub(crate) fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
pub(crate) fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    reveal(h, label);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(4);
}
pub(crate) fn wheel(h: &mut Harness<'_, ViamindApp>, at: egui::Pos2, delta: f32) {
    h.hover_at(at);
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        phase: egui::TouchPhase::Move,
        delta: vec2(0.0, delta),
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(8);
}

pub(crate) fn original(h: &mut Harness<'_, ViamindApp>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json");
    input(h, "Package file path (load or NEW save)", &path.to_string_lossy());
    click(h, "Load complete package file");
    wait(h, "Loaded 5 units");
    click(h, "Package unit to edit");
    h.get_by_label("P (Power)").click();
    h.run_steps(5);
}
