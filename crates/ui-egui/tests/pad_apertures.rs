use egui::{Event, Key, Modifiers};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn setup() -> (EngineHandle, Harness<'static, ViamindApp>) {
    let e = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap());
    let part = e.read(|p| p.board.parts.values().next().unwrap().clone());
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_edit_pad_apertures(part);
    let mut h = Harness::builder().with_size(egui::vec2(640., 400.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(6);
    (e, h)
}
fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(10);
    let r = h.get_by_label(label).rect();
    h.hover_at(r.center());
    h.run_steps(2);
    let r = h.get_by_label(label).rect();
    let clip = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| {
            let matching = match &s.shape {
                egui::Shape::Text(t) => t.galley.text() == label,
                egui::Shape::Rect(b) => {
                    b.rect.contains(r.center()) && (b.rect.width() - r.width()).abs() <= 4. && (b.rect.height() - r.height()).abs() <= 4.
                }
                _ => false,
            };
            matching.then_some(s.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or_else(|| panic!("missing paint for {label}"));
    assert!(clip.contains_rect(r), "{label} rect{r:?}, effectiveclip{clip:?}");
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    reveal(h, label);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(3);
}
#[test]
fn actual_compact_exact_policy_apply_undo_and_cancel_preserve_copper_and_metadata() {
    let (e, mut h) = setup();
    let before = e.session_state();
    click(&mut h, "Explicit pad openings");
    click(&mut h, "Mask rectangle");
    click(&mut h, "Paste rectangle");
    for (l, v) in [("Mask width", "1.000003"), ("Mask height", "2.000007"), ("Paste width", "0.800001"), ("Paste height", "1.500003")] {
        input(&mut h, l, v);
    }
    assert_eq!(e.session_state(), before);
    for label in ["Apply pad openings", "Reload pad openings", "Cancel"] {
        reveal(&mut h, label);
    }
    click(&mut h, "Apply pad openings");
    let after = e.session_state();
    let part = after.project.board.parts.values().next().unwrap();
    let a = part.pads.as_ref().unwrap().0[0].apertures.as_ref().unwrap();
    assert_eq!(
        (a.mask.unwrap().width.0, a.mask.unwrap().height.0, a.paste.unwrap().width.0, a.paste.unwrap().height.0),
        (1_000_003, 2_000_007, 800_001, 1_500_003)
    );
    let mut expected = before.project.clone();
    expected.board.parts.get_mut(&part.id).unwrap().pads.as_mut().unwrap().0[0].apertures = Some(a.clone());
    assert_eq!(after.project, expected);
    assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), expected);
    h.state_mut().begin_edit_pad_apertures(part.clone());
    h.run_steps(5);
    input(&mut h, "Mask width", "9");
    let unchanged = e.session_state();
    click(&mut h, "Cancel");
    assert_eq!(e.session_state(), unchanged);
}
#[test]
fn actual_compact_signed_margin_is_explicit_and_stale_apply_preserves_draft_until_reload() {
    let (e, mut h) = setup();
    let before = e.session_state();
    click(&mut h, "Explicit pad openings");
    input(&mut h, "Equal margin calculator (signed mm)", "-0.000001");
    click(&mut h, "Calculate Mask dimensions from copper");
    let retained = h.get_by_label("Mask width").accesskit_node().id();
    e.execute("add_part", serde_json::json!({"designator":"PEER1","value":"peer","footprint":"test","x_mm":1,"y_mm":1}), Origin::agent("peer"))
        .unwrap();
    h.run_steps(4);
    let peer = e.session_state();
    click(&mut h, "Apply pad openings");
    assert_eq!(e.session_state(), peer);
    assert_eq!(h.get_by_label("Mask width").accesskit_node().id(), retained);
    assert!(h.query_by_label_contains("session changed").is_some());
    click(&mut h, "Reload pad openings");
    assert!(h.query_by_label("Mask width").is_none());
    click(&mut h, "Explicit pad openings");
    click(&mut h, "Apply pad openings");
    let now = e.session_state();
    let a = now.project.board.parts[before.project.board.parts.keys().next().unwrap()].pads.as_ref().unwrap().0[0].apertures.as_ref().unwrap();
    assert!(a.mask.is_none() && a.paste.is_none());
}

#[test]
fn actual_compact_detached_policy_fields_publish_library3_without_applying_to_design() {
    let engine = EngineHandle::new(viamind_doc::Project::new("Keep detached"));
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_definition();
    let mut h = Harness::builder().with_size(egui::vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(6);
    input(&mut h, "Footprint definition name", "Exact apertures");
    click(&mut h, "Add blank rectangular pad");
    for (label, value) in [
        ("Pad 1 number", "1"),
        ("Pad 1 X (mm)", "-0.500003"),
        ("Pad 1 Y (mm)", "0.250001"),
        ("Pad 1 width (mm)", "1.200003"),
        ("Pad 1 height (mm)", "0.800001"),
    ] {
        input(&mut h, label, value);
    }
    click(&mut h, "Edit definition pad 1 openings");
    click(&mut h, "Explicit pad openings");
    click(&mut h, "Mask rectangle");
    click(&mut h, "Paste rectangle");
    for (label, value) in [("Mask width", "1.300005"), ("Mask height", "0.900003"), ("Paste width", "1.100001"), ("Paste height", "0.700003")] {
        input(&mut h, label, value);
    }
    let expected = h.state().footprint_definition.definition().unwrap();
    let policy = expected.pads[0].apertures.as_ref().unwrap();
    assert_eq!((policy.mask.unwrap().width.0, policy.paste.unwrap().height.0), (1_300_005, 700_003));
    let id = h.get_by_label("Paste height").accesskit_node().id();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("explicit.json");
    input(&mut h, "NEW footprint definition path", path.to_str().unwrap());
    click(&mut h, "Save NEW footprint definition");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().footprint_definition.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(5);
    assert!(!h.state().footprint_definition.busy());
    assert_eq!(viamind_doc::footprint::load(&path).unwrap(), expected);
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap()["version"], 3);
    reveal(&mut h, "Paste height");
    assert_eq!(h.get_by_label("Paste height").accesskit_node().id(), id);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    click(&mut h, "Close footprint definition");
    h.state_mut().begin_footprint_definition();
    h.run_steps(5);
    assert_eq!(h.state().footprint_definition.definition().unwrap(), expected);
    assert_eq!(engine.session_state(), before);
}

#[test]
fn actual_detached_opening_text_identity_and_cursor_survive_earlier_pad_removal() {
    let engine = EngineHandle::new(viamind_doc::Project::new("Keep detached identity"));
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.json");
    let policy = serde_json::json!({"mask":{"width":1300005,"height":900003},"paste":{"width":1100001,"height":700003}});
    let definition = serde_json::json!({"name":"Two independent pads","pads":[
        {"number":"1","position":{"x":-2000000,"y":0},"width":1200003,"height":800001,"apertures":policy},
        {"number":"2","position":{"x":2000000,"y":0},"width":1200003,"height":800001,"apertures":policy}]});
    viamind_ui_egui::actions::UiAction::SaveFootprintDefinition.run(&engine, serde_json::json!({"path":path,"footprint":definition})).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_definition();
    let mut h = Harness::builder().with_size(egui::vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(6);
    input(&mut h, "Native footprint source path", path.to_str().unwrap());
    click(&mut h, "Load native footprint definition");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().footprint_definition.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(5);
    click(&mut h, "Edit definition pad 2 openings");
    input(&mut h, "Paste height", "0.700009");
    let id = h.get_by_label("Paste height").accesskit_node().id();
    let focused = h.ctx.memory(egui::Memory::focused).unwrap();
    // Save the actual text cursor selection, not only an accesskit identity.
    let selection = egui::TextEdit::load_state(&h.ctx, focused).unwrap().cursor.char_range();
    click(&mut h, "Remove definition pad 1");
    reveal(&mut h, "Paste height");
    h.get_by_label("Paste height").focus();
    h.run_steps(3);
    assert_eq!(h.get_by_label("Paste height").accesskit_node().id(), id);
    assert_eq!(h.ctx.memory(egui::Memory::focused), Some(focused));
    assert_eq!(egui::TextEdit::load_state(&h.ctx, focused).unwrap().cursor.char_range(), selection);
    let value = h.state().footprint_definition.definition().unwrap();
    assert_eq!(value.pads.len(), 1);
    assert_eq!(value.pads[0].number, "2");
    assert_eq!(value.pads[0].apertures.as_ref().unwrap().paste.unwrap().height.0, 700009);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
}
