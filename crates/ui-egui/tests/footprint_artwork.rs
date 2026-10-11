use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;

fn setup(size: egui::Vec2) -> (EngineHandle, Harness<'static, ViamindApp>) {
    let e = EngineHandle::new(Project::new("Artwork input"));
    let id = e.execute("add_part", json!({"designator":"U1","value":"IC","footprint":"Test","x_mm":10,"y_mm":20}), Origin::User).unwrap().created[0];
    let part = e.read(|p| p.board.parts[&id].clone());
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_edit_footprint_artwork(part);
    let mut h = Harness::builder().with_size(size).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(6);
    (e, h)
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(4);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(3);
}
#[test]
fn actual_compact_exact_author_apply_one_undo_and_local_cancel() {
    let (e, mut h) = setup(vec2(640., 400.));
    let before = e.session_state();
    click(&mut h, "Explicit footprint artwork");
    click(&mut h, "Add blank silkscreen line");
    for (label, value) in [
        ("Silk 1 Start X", "-0.000003"),
        ("Silk 1 Start Y", "0.000005"),
        ("Silk 1 End X", "0.000007"),
        ("Silk 1 End Y", "0.000011"),
        ("Silk 1 Stroke width", "0.000003"),
    ] {
        input(&mut h, label, value);
    }
    assert_eq!(e.session_state(), before);
    for label in ["Apply artwork", "Reload artwork", "Cancel"] {
        let r = h.get_by_label(label).rect();
        assert!(h.ctx.content_rect().contains_rect(r), "{label}: {r:?}");
    }
    click(&mut h, "Apply artwork");
    let after = e.read(Clone::clone);
    let part = after.board.parts.values().next().unwrap();
    let line = &part.artwork.as_ref().unwrap().silkscreen[0];
    assert_eq!((line.start.x.0, line.start.y.0, line.end.x.0, line.end.y.0, line.width.0), (-3, 5, 7, 11, 3));
    let mut expected = before.project.board.parts[&part.id].clone();
    expected.artwork = part.artwork.clone();
    assert_eq!(*part, expected);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after);
    h.state_mut().begin_edit_footprint_artwork(part.clone());
    h.run_steps(5);
    input(&mut h, "Silk 1 Start X", "1");
    let unchanged = e.session_state();
    click(&mut h, "Cancel");
    assert_eq!(e.session_state(), unchanged);
}
#[test]
fn actual_stable_row_identity_and_stale_peer_refusal() {
    let (e, mut h) = setup(vec2(1000., 1000.));
    click(&mut h, "Explicit footprint artwork");
    click(&mut h, "Add blank silkscreen line");
    click(&mut h, "Add blank silkscreen line");
    h.get_by_label("Silk 2 Start X").scroll_to_me();
    h.run_steps(4);
    let id = h.get_by_label("Silk 2 Start X").accesskit_node().id();
    click(&mut h, "Remove silkscreen line 1");
    assert_eq!(h.get_by_label("Silk 1 Start X").accesskit_node().id(), id);
    for (label, value) in
        [("Silk 1 Start X", "0"), ("Silk 1 Start Y", "0"), ("Silk 1 End X", "1"), ("Silk 1 End Y", "0"), ("Silk 1 Stroke width", "0.12")]
    {
        input(&mut h, label, value);
    }
    e.execute("add_part", json!({"designator":"R2","value":"peer","footprint":"Test","x_mm":2,"y_mm":3}), Origin::agent("peer")).unwrap();
    h.run_steps(4);
    let before = e.session_state();
    click(&mut h, "Apply artwork");
    assert_eq!(e.session_state(), before);
    assert!(h.query_by_label_contains("session changed; read get_session_state").is_some());
    click(&mut h, "Cancel");
}

#[test]
fn explicit_courtyard_layers_follow_live_edits_and_undo_without_changing_legacy_defaults() {
    use viamind_ui_egui::layers::LayerId;
    let (e, mut h) = setup(vec2(1000., 1000.));
    click(&mut h, "Cancel");
    let legacy = h.state().layers.clone();
    assert!(legacy.get(LayerId::F_COURTYARD).is_none() && legacy.get(LayerId::B_COURTYARD).is_none());
    let before = e.session_state();
    let expected = before.project.board.parts.values().next().unwrap();
    e.execute("set_footprint_artwork", json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"expected":expected,"artwork":{"silkscreen":[],"courtyard":{"min":{"x":-2000003,"y":-3000005},"max":{"x":4000007,"y":5000011},"width":100001}}}), Origin::User).unwrap();
    h.run_steps(6);
    assert!(h.state().layers.get(LayerId::F_COURTYARD).is_some());
    assert!(h.state().layers.get(LayerId::B_COURTYARD).is_none());
    e.undo(Origin::User).unwrap();
    h.run_steps(6);
    assert_eq!(h.state().layers, legacy);
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    h.run_steps(6);
    assert!(h.state().layers.get(LayerId::F_COURTYARD).is_some());
}
