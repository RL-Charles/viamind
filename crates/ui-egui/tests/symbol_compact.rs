//! Compact input must hit painted controls, not merely virtualized accessibility nodes.
use egui::{Rect, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use viamind_engine::EngineHandle;
use viamind_ui_egui::ViamindApp;

fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
}
fn wait(h: &mut Harness<'_, ViamindApp>, label: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while h.query_by_label_contains(label).is_none() {
        assert!(std::time::Instant::now() < end, "missing {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
}
fn harness(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_symbol_library();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    wait(&mut h, "Target: Power regulator");
    h
}
fn painted_clip(h: &Harness<'_, ViamindApp>, rect: Rect) -> Rect {
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
fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) -> Rect {
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
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    reveal(h, label);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(4);
}
fn wheel(h: &mut Harness<'_, ViamindApp>, at: egui::Pos2, delta: f32) {
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

#[test]
fn compact_complete_pin_row_metadata_actions_errors_and_new_save_are_painted_and_exact() {
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let mut h = harness(&e);
    click(&mut h, "New detached definition");
    for (label, value) in [
        ("Definition name", "Compact original symbol"),
        ("Definition default value", "Original value"),
        ("Definition footprint (optional)", "Original footprint"),
        ("Definition width (mm)", "10.000003"),
        ("Definition height (mm)", "8.000007"),
    ] {
        input(&mut h, label, value);
    }
    click(&mut h, "Add definition pin");
    for (label, value) in [("Pin 2 number", "VIN"), ("Pin 2 name", "Retained input"), ("Pin 2 X (mm)", "1.000003"), ("Pin 2 Y (mm)", "-2.000007")] {
        input(&mut h, label, value);
    }
    click(&mut h, "Pin 2 electrical type");
    click(&mut h, "Input");
    let fields = ["number", "name", "X (mm)", "Y (mm)", "electrical type"];
    let ids: Vec<_> = fields.iter().map(|field| h.get_by_label(&format!("Pin 2 {field}")).accesskit_node().id()).collect();
    reveal(&mut h, "Remove pin 2");
    let union = fields.iter().map(|field| h.get_by_label(&format!("Pin 2 {field}")).rect()).fold(h.get_by_label("Remove pin 2").rect(), Rect::union);
    assert!(
        painted_clip(&h, h.get_by_label("Remove pin 2").rect()).contains_rect(union),
        "complete pin row must fit one actual compact clip: {union:?}"
    );
    h.get_by_label("Pin 2 name").focus();
    h.run_steps(3);
    let text_id = h.ctx.memory(egui::Memory::focused).unwrap();
    let cursor = egui::text_edit::TextEditState::load(&h.ctx, text_id).unwrap().cursor.char_range();
    click(&mut h, "Remove pin 1");
    assert_eq!(egui::text_edit::TextEditState::load(&h.ctx, text_id).unwrap().cursor.char_range(), cursor);
    for (i, field) in fields.iter().enumerate() {
        assert_eq!(h.get_by_label(&format!("Pin 1 {field}")).accesskit_node().id(), ids[i]);
    }
    click(&mut h, "Save NEW definition file");
    wait(&mut h, "Enter a NEW definition file path");
    h.get_by_label("Enter a NEW definition file path").scroll_to_me();
    h.run_steps(8);
    let error = h.get_by_label("Enter a NEW definition file path").rect();
    let clip = h
        .output()
        .shapes
        .iter()
        .find_map(|shape| {
            let egui::Shape::Text(text) = &shape.shape else { return None };
            (text.galley.text() == "Enter a NEW definition file path").then_some(shape.clip_rect.intersect(h.ctx.content_rect()))
        })
        .expect("painted error text");
    assert!(clip.contains_rect(error));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("compact-original.json");
    input(&mut h, "NEW definition file path", &path.to_string_lossy());
    click(&mut h, "Use edited definition");
    assert_eq!(e.session_state(), before);
    click(&mut h, "Save NEW definition file");
    wait(&mut h, "Created detached definition");
    let saved = viamind_doc::symbol_library::load(&path).unwrap();
    assert_eq!(saved.width, viamind_geom::Nm(10_000_003));
    assert_eq!(saved.height, viamind_geom::Nm(8_000_007));
    assert_eq!(saved.pins.len(), 1);
    assert_eq!(saved.pins[0].number, "VIN");
    assert_eq!(saved.pins[0].position.x, viamind_geom::Nm(1_000_003));
    assert_eq!(saved.pins[0].position.y, viamind_geom::Nm(-2_000_007));
    assert_eq!(saved.pins[0].electrical, viamind_doc::schematic::PinType::Input);
    let bytes = std::fs::read(&path).unwrap();
    click(&mut h, "Save NEW definition file");
    wait(&mut h, "already exists");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    click(&mut h, "Cancel definition edits");
    assert!(h.query_by_label("Definition name").is_none());
    for label in [
        "Symbol library file path",
        "New symbol reference",
        "Symbol X (mm)",
        "Symbol Y (mm)",
        "Library source symbol",
        "Library template name",
        "Export new symbol template file",
    ] {
        reveal(&mut h, label);
    }
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}

#[test]
fn compact_large_pin_list_scrolls_real_wheel_with_bounded_rows_and_stable_fields() {
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let mut template =
        viamind_doc::symbol_library::from_slice(include_bytes!("../../../fixtures/symbols/original-regulator.viamind-symbol.json")).unwrap();
    template.pins = (1..=1024)
        .map(|n| viamind_doc::symbol_library::TemplatePin {
            number: n.to_string(),
            name: format!("Original pin {n}"),
            position: viamind_doc::schematic::Coordinate { x: viamind_geom::Nm(0), y: viamind_geom::Nm(0) },
            electrical: viamind_doc::schematic::PinType::Passive,
        })
        .collect();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original-1024.json");
    viamind_doc::symbol_library::save_new(&template, &path, None).unwrap();
    let mut h = harness(&e);
    input(&mut h, "Symbol library file path", &path.to_string_lossy());
    click(&mut h, "Load symbol template");
    wait(&mut h, "Loaded Original regulator");
    click(&mut h, "Edit loaded definition");
    let first = reveal(&mut h, "Pin 1 name");
    let id = h.get_by_label("Pin 1 name").accesskit_node().id();
    let rendered = h.query_all_by_label_contains("X (mm)").count();
    assert!(rendered <= 6, "1024 pins must keep at most five rendered pin rows plus placement X: {rendered}");
    wheel(&mut h, first.center(), -1_000_000.0);
    assert!(h.query_by_label("Pin 1024 name").is_some(), "real inner wheel must reach last virtual row");
    input(&mut h, "Pin 1024 name", "Last original pin");
    let last = reveal(&mut h, "Pin 1024 electrical type");
    wheel(&mut h, last.center(), 1_000_000.0);
    reveal(&mut h, "Pin 1 name");
    assert_eq!(h.get_by_label("Pin 1 name").accesskit_node().id(), id);
    assert!(h.query_all_by_label_contains("X (mm)").count() <= 6);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
