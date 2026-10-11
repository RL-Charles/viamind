use egui::{Event, Key, Modifiers, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/package-annotation.viamind.json")).unwrap())
}
fn setup(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 1800.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    click(&mut h, "Annotate symbols");
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, text: &str) {
    h.get_by_label("Annotation starting number").focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label("Annotation starting number").type_text(text);
    h.run_steps(5);
}
fn painted(h: &Harness<'_, ViamindApp>, label: &str) -> Rect {
    let r = h.get_by_label(label).rect();
    assert!(h.ctx.content_rect().contains_rect(r), "{label}: {r:?}");
    let clip = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| {
            let egui::Shape::Rect(background) = &s.shape else {
                return None;
            };
            let b = background.rect;
            (b.contains(r.center()) && (b.width() - r.width()).abs() <= 4.0 && (b.height() - r.height()).abs() <= 4.0)
                .then_some(s.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("actual painted button");
    assert!(clip.contains_rect(r), "{label}: {r:?} outside painted clip {clip:?}");
    r
}
#[test]
fn actual_group_preview_apply_and_one_undo_preserve_complete_package_membership() {
    let e = engine();
    let original = e.read(Clone::clone);
    let mut h = setup(&e);
    click(&mut h, "Renumber all symbols");
    input(&mut h, "5");
    let before = e.session_state();
    click(&mut h, "Prepare annotation");
    assert_eq!(e.session_state(), before);
    assert!(h.query_by_label("12 symbols/units, 2 packages, 12 proposed member reference changes").is_some());
    assert!(h.query_by_label_contains("U? to U5 - unit A").is_some());
    click(&mut h, "Apply annotation");
    let changed = e.read(Clone::clone);
    let c = changed.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
    for group in c.packages.values() {
        let references: std::collections::BTreeSet<_> = group.units.iter().map(|u| &c.symbols[&u.symbol_id].designator).collect();
        assert_eq!(references.len(), 1);
    }
    assert_eq!(c.packages, original.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().packages);
    assert_eq!(changed.board, original.board);
    assert_eq!(e.session_state().cursor.sequence, before.cursor.sequence + 1);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), original);
}
#[test]
fn compact_painted_actions_and_virtualized_last_member_scrolling_remain_reachable() {
    let e = engine();
    let mut h = setup(&e);
    click(&mut h, "Renumber all symbols");
    input(&mut h, "5");
    click(&mut h, "Prepare annotation");
    h.set_size(vec2(640.0, 400.0));
    h.run_steps(12);
    let before = e.session_state();
    for label in ["Prepare annotation", "Apply annotation", "Cancel annotation preview"] {
        painted(&h, label);
        assert!(!h.get_by_label(label).accesskit_node().is_disabled());
    }
    let rows = h.query_all_by_label_contains("U? to U").collect::<Vec<_>>();
    assert!(!rows.is_empty() && rows.len() < 12, "rendered {} of12", rows.len());
    let at = rows[0].rect().center();
    drop(rows);
    h.event(Event::PointerMoved(at));
    h.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -1000.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(10);
    assert!(h.query_by_label_contains("U? to U8 - single").is_some());
    for label in ["Prepare annotation", "Apply annotation", "Cancel annotation preview"] {
        painted(&h, label);
    }
    click(&mut h, "Cancel annotation preview");
    assert!(h.query_by_label("12 symbols/units, 2 packages, 12 proposed member reference changes").is_none());
    assert_eq!(e.session_state(), before);
}
#[test]
fn peer_cursor_and_same_uuid_reopen_disable_stale_group_apply_without_mutation() {
    let e = engine();
    let mut h = setup(&e);
    click(&mut h, "Renumber all symbols");
    click(&mut h, "Prepare annotation");
    e.execute("add_part", json!({"designator":"PEER1","value":"peer","footprint":"x","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    h.run_steps(5);
    let peer = e.session_state();
    assert!(h.get_by_label("Apply annotation").accesskit_node().is_disabled());
    click(&mut h, "Apply annotation");
    assert_eq!(e.session_state(), peer);
    assert!(h.query_by_label_contains("prepare annotation again to reload").is_some());
    click(&mut h, "Prepare annotation");
    assert!(!h.get_by_label("Apply annotation").accesskit_node().is_disabled());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    h.run_steps(5);
    let reopened = e.session_state();
    assert_eq!(reopened.project.id, peer.project.id);
    assert_ne!(reopened.cursor.stream_id, peer.cursor.stream_id);
    assert!(h.query_by_label("Apply annotation").is_none());
    assert_eq!(e.session_state(), reopened);
}
#[test]
fn option_or_text_changes_and_explicit_cancel_only_discard_local_proposal() {
    let e = engine();
    let mut h = setup(&e);
    let before = e.session_state();
    let id = h.get_by_label("Annotation starting number").accesskit_node().id();
    click(&mut h, "Renumber all symbols");
    click(&mut h, "Prepare annotation");
    input(&mut h, "8");
    assert!(h.get_by_label("Apply annotation").accesskit_node().is_disabled());
    click(&mut h, "Prepare annotation");
    click(&mut h, "Sort symbols by Y then X");
    assert!(h.get_by_label("Apply annotation").accesskit_node().is_disabled());
    click(&mut h, "Prepare annotation");
    click(&mut h, "Cancel annotation preview");
    assert!(h.get_by_label("Apply annotation").accesskit_node().is_disabled());
    assert_eq!(h.get_by_label("Annotation starting number").accesskit_node().id(), id);
    assert_eq!(h.get_by_label("Annotation starting number").accesskit_node().value().as_deref(), Some("8"));
    assert_eq!(e.session_state(), before);
}
