//! Actual compact input with painted effective-clip oracles.
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::EngineHandle;

fn app() -> (ViamindApp, EngineHandle) {
    let project = viamind_doc::file::from_str(include_str!("../../../../fixtures/schematic/regulator.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    (app, engine)
}
fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) -> egui::Rect {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(10);
    let rect = h.get_by_label(label).rect();
    h.hover_at(rect.center());
    h.run_steps(2);
    let rect = h.get_by_label(label).rect();
    assert_button_clip(h, label, rect);
    rect
}
fn assert_button_clip(h: &Harness<'_, ViamindApp>, label: &str, rect: egui::Rect) {
    let clip = h
        .output()
        .shapes
        .iter()
        .filter_map(|shape| {
            let egui::Shape::Rect(background) = &shape.shape else { return None };
            (background.rect.contains(rect.center())
                && (background.rect.width() - rect.width()).abs() <= 4.0
                && (background.rect.height() - rect.height()).abs() <= 4.0)
                .then_some(shape.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or_else(|| panic!("missing painted button {label}: {rect:?}"));
    assert!(clip.contains_rect(rect), "{label}: response {rect:?}, actual effective clip {clip:?}");
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
#[test]
fn compact_sidebar_baseline_reveals_supported_entries_without_session_changes() {
    let (app, engine) = app();
    let before = engine.session_state();
    let status = engine.document_status();
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    for label in [
        "Multi-unit package library…",
        "Native symbol library…",
        "Electrical rule check…",
        "Create schematic sheet",
        "Rename sheet",
        "Delete sheet",
        "Reload sheet details",
        "Add symbol",
        "Add wire",
        "Add junction",
        "Add label",
        "Fit schematic",
        "Draw wires on canvas",
        "Annotate symbols",
    ] {
        reveal(&mut h, label);
    }
    click(&mut h, "Fit schematic");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
#[test]
fn compact_sidebar_baseline_draft_actions_fit_effective_clip() {
    let (app, engine) = app();
    let before = engine.session_state();
    let status = engine.document_status();
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Add symbol");
    for label in ["Apply schematic object", "Reload schematic draft", "Cancel schematic draft"] {
        reveal(&mut h, label);
    }
    click(&mut h, "Cancel schematic draft");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}

#[test]
fn compact_sidebar_actual_wheel_reveals_footer_without_zooming_canvas() {
    let (app, engine) = app();
    let before = engine.session_state();
    let status = engine.document_status();
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    let camera = h.state().schematic.camera;
    assert!(!h.ctx.content_rect().contains_rect(h.get_by_label("Annotate symbols").rect()), "footer starts below the fold");
    for _ in 0..6 {
        h.event(egui::Event::PointerMoved(egui::pos2(100.0, 120.0)));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -300.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(8);
    }
    assert_button_clip(&h, "Annotate symbols", h.get_by_label("Annotate symbols").rect());
    assert_eq!(h.state().schematic.camera, camera, "sidebar wheel must not zoom canvas");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
#[test]
fn compact_sidebar_retained_pin_field_identity_survives_other_row_removal() {
    let (app, engine) = app();
    let before = engine.session_state();
    let status = engine.document_status();
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    let viewport_before = h.state().schematic.viewport;
    click(&mut h, "Add symbol");
    click(&mut h, "Add pin");
    h.get_by_label("Pin 2 X (mm)").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Pin 2 X (mm)").focus();
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label("Pin 2 X (mm)").type_text("1.000003");
    h.run_steps(3);
    let original_id = h.ctx.memory(egui::Memory::focused).expect("focused second pin coordinate");
    let original_cursor = egui::text_edit::TextEditState::load(&h.ctx, original_id).unwrap().cursor.char_range();
    assert_eq!(h.state().schematic.draft.as_ref().unwrap().pins[1].x, "1.000003");
    println!("compact sidebar canvas before draft {viewport_before:?}; with draft {:?}", h.state().schematic.viewport);
    h.get_all_by_label("Remove pin").next().unwrap().scroll_to_me();
    h.run_steps(10);
    let rect = h.get_all_by_label("Remove pin").next().unwrap().rect();
    h.hover_at(rect.center());
    h.run_steps(2);
    assert_button_clip(&h, "first Remove pin", h.get_all_by_label("Remove pin").next().unwrap().rect());
    h.get_all_by_label("Remove pin").next().unwrap().click();
    h.run_steps(5);
    assert_eq!(h.state().schematic.draft.as_ref().unwrap().pins.len(), 1);
    h.get_by_label("Pin 1 X (mm)").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Pin 1 X (mm)").focus();
    h.run_steps(2);
    let retained_id = h.ctx.memory(egui::Memory::focused).expect("focused retained pin coordinate");
    assert_eq!(retained_id, original_id, "retained row text state must not acquire the removed row's identity");
    assert_eq!(egui::text_edit::TextEditState::load(&h.ctx, retained_id).unwrap().cursor.char_range(), original_cursor);
    assert_eq!(h.state().schematic.draft.as_ref().unwrap().pins[0].x, "1.000003");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}

fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(10);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
#[test]
fn compact_sidebar_local_drafts_and_exact_apply_keep_existing_atomic_undo() {
    let (app, engine) = app();
    let before = engine.session_state();
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    for (label, kind) in [("Add symbol", "symbol"), ("Add wire", "wire"), ("Add junction", "junction"), ("Add label", "label")] {
        click(&mut h, label);
        assert_eq!(h.state().schematic.draft.as_ref().unwrap().kind, kind);
        click(&mut h, "Cancel schematic draft");
        assert_eq!(engine.session_state(), before);
    }
    click(&mut h, "Add symbol");
    input(&mut h, "Reference", "U77");
    input(&mut h, "Schematic X (mm)", "12.000003");
    input(&mut h, "Schematic Y (mm)", "-0.000007");
    input(&mut h, "Pin 1 name", "EXACT");
    assert_eq!(engine.session_state(), before);
    click(&mut h, "Apply schematic object");
    let after = engine.session_state();
    let symbol = after
        .project
        .schematic
        .sheets
        .values()
        .flat_map(|sheet| sheet.content.iter().flat_map(|c| c.symbols.values()))
        .find(|s| s.designator == "U77")
        .unwrap();
    assert_eq!((symbol.position.x.0, symbol.position.y.0), (12_000_003, -7));
    assert_eq!(symbol.pins[0].name, "EXACT");
    assert_eq!(after.project.board, before.project.board);
    assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: Key::Z, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(3);
    }
    assert_eq!(engine.session_state().project, before.project);
    for pressed in [true, false] {
        h.event(egui::Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        });
        h.run_steps(3);
    }
    assert_eq!(engine.session_state().project, after.project);
}
