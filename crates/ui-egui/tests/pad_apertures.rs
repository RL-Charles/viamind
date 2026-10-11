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
