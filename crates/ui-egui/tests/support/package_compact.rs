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
    if (label.starts_with("Pin ") || label.starts_with("Remove pin ")) && h.query_by_label(label).is_none() {
        let at = h.query_by_label_contains("/ 1024 ordered pins").unwrap().rect().center() + vec2(0., 60.);
        for _ in 0..12 {
            if h.query_by_label(label).is_some() {
                break;
            }
            wheel(h, at, -120.);
        }
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

/// Author every unit and every pin through actual exposed widgets, without file loading.
pub(crate) fn authored(h: &mut Harness<'_, ViamindApp>) {
    input(h, "Package definition name", "Original quad amplifiers and power");
    input(h, "Package default value", "Authored fourteen pins");
    input(h, "Package footprint (optional)", "Authored-14pad");
    for _ in 0..3 {
        click(h, "Add package unit");
    }
    // The initial local editor has explicit A/B units and adds C/D/E in order.
    for i in 0..5 {
        let key = char::from(b'A' + u8::try_from(i).unwrap());
        click(h, "Package unit to edit");
        h.get_by_label(&format!("{key} (Unit {key})")).click();
        h.run_steps(5);
        let power = i == 4;
        if power {
            input(h, "Unit key", "P");
        }
        input(h, "Unit display name", &if power { "Power".to_string() } else { format!("Amplifier {key}") });
        click(h, if power { "Power unit" } else { "Functional unit" });
        input(h, "Definition width (mm)", &if power { "4.000003".to_string() } else { format!("6.{:06}", 3 + 2 * i) });
        input(h, "Definition height (mm)", if power { "6.000007" } else { "4.000007" });
        for _ in 0..if power { 1 } else { 2 } {
            click(h, "Add definition pin");
        }
        for j in 0..if power { 2 } else { 3 } {
            let row = j + 1;
            input(h, &format!("Pin {row} number"), &(if power { 13 + j } else { i * 3 + row }).to_string());
            input(h, &format!("Pin {row} name"), if power { ["V+", "V-"][j] } else { ["IN+", "IN-", "OUT"][j] });
            input(
                h,
                &format!("Pin {row} X (mm)"),
                if power {
                    "0.000007"
                } else if j == 2 {
                    "3.000003"
                } else {
                    "-3.000003"
                },
            );
            input(h, &format!("Pin {row} Y (mm)"), if power { ["-4.000003", "4.000003"][j] } else { ["-1.000007", "0", "1.000007"][j] });
            click(h, &format!("Pin {row} electrical type"));
            click(
                h,
                if power {
                    "PowerInput"
                } else if j == 2 {
                    "Output"
                } else {
                    "Input"
                },
            );
        }
    }
}
