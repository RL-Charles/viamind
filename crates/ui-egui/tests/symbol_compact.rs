//! Actual compact input and ancestor-clip regressions.
#[path = "support/symbol_compact.rs"]
mod prepare;
use egui::Rect;
use egui_kittest::kittest::{NodeT, Queryable};
use prepare::*;

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
    for _ in 0..3 {
        let button = h.get_by_label("Remove pin 2").rect();
        let union = fields.iter().map(|field| h.get_by_label(&format!("Pin 2 {field}")).rect()).fold(button, Rect::union);
        let clip = painted_clip(&h, button);
        if clip.contains_rect(union) {
            break;
        }
        // Individual ScrollIntoView reveals its target, not the full row.
        // Ordinary wheel input centers the complete row within its real clip.
        wheel(&mut h, button.center(), clip.center().y - union.center().y);
    }
    let button = h.get_by_label("Remove pin 2").rect();
    let union = fields.iter().map(|field| h.get_by_label(&format!("Pin 2 {field}")).rect()).fold(button, Rect::union);
    let clip = painted_clip(&h, button);
    assert!(clip.contains_rect(union), "complete pin row must fit one actual compact clip: {union:?}, {clip:?}");
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
    wait(&mut h, "File exists");
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
