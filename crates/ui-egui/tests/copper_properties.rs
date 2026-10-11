//! Actual pointer selection and accessible edits use shared guarded copper commands.
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Project, Track, Via};
use viamind_engine::{EngineHandle, Origin, SetSelection};
use viamind_geom::{Nm, Point, Polygon};
use viamind_ui_egui::{ViamindApp, camera::WorldPos, layers::LayerId, scene::Prim};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn setup(size: egui::Vec2) -> (EngineHandle, Harness<'static, ViamindApp>, ObjectId, ObjectId) {
    let mut project = Project::new("Copper properties UI");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(50_000_000, 0), p(50_000_000, 50_000_000), p(0, 50_000_000)]), vec![]).unwrap());
    let t = Track {
        id: ObjectId::new(),
        start: p(5_000_003, 10_000_007),
        end: p(15_000_001, 10_000_007),
        width: Nm(250_000),
        layer: CopperLayer::Top,
        net: Some("N".into()),
    };
    let v = Via { id: ObjectId::new(), position: p(30_000_001, 25_000_003), diameter: Nm(1_000_000), drill: Nm(300_000), net: Some("N".into()) };
    let (tid, vid) = (t.id, v.id);
    project.board.tracks.insert(tid, t);
    project.board.vias.insert(vid, v);
    let e = EngineHandle::new(project);
    let mut h =
        Harness::builder().with_size(size).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    (e, h, tid, vid)
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(4);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn key(h: &mut Harness<'_, ViamindApp>, key: Key) {
    for pressed in [true, false] {
        h.event(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}
fn open(e: &EngineHandle, h: &mut Harness<'_, ViamindApp>, id: ObjectId) {
    e.set_selection(SetSelection { ids: vec![id], mode: Default::default() }, Origin::User).unwrap();
    h.state_mut().begin_copper_properties();
    h.run_steps(5);
}
#[test]
fn pointer_selected_track_menu_exact_local_edit_atomic_apply_and_one_undo() {
    let (e, mut h, tid, _) = setup(vec2(1600., 1000.));
    let pos = h.state().canvas.camera.world_to_screen(WorldPos::new(10., 10.), h.state().canvas.viewport);
    for pressed in [true, false] {
        h.event(Event::PointerMoved(pos));
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    assert_eq!(e.with_engine(|e| e.selection().ids.clone()), vec![tid]);
    click(&mut h, "Board");
    click(&mut h, "Edit selected copper properties…");
    let before = e.session_state();
    let id = h.get_by_label("Property track width (mm)").accesskit_node().id();
    input(&mut h, "Property track width (mm)", "0.350001");
    click(&mut h, "Property bottom copper");
    input(&mut h, "Property net (empty = unassigned)", "EXACT N");
    assert_eq!(e.session_state(), before);
    click(&mut h, "Apply properties");
    let after = e.read(Clone::clone);
    let t = &after.board.tracks[&tid];
    assert_eq!(t.width, Nm(350001));
    assert_eq!(t.layer, CopperLayer::Bottom);
    assert_eq!(t.net.as_deref(), Some("EXACT N"));
    assert_eq!((t.start, t.end), (before.project.board.tracks[&tid].start, before.project.board.tracks[&tid].end));
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    assert_eq!(h.get_by_label("Property track width (mm)").accesskit_node().id(), id);
    assert!(!h.get_by_label("Apply properties").accesskit_node().is_disabled());
    assert!(h.state().scene.items.iter().any(|i| i.layer == LayerId::B_CU && matches!(i.prim,Prim::Segment{width,..}if(width-0.350001).abs()<1e-12)));
    e.undo(Origin::User).unwrap();
    h.run_steps(4);
    assert_eq!(e.read(Clone::clone), before.project);
    assert!(h.get_by_label("Apply properties").accesskit_node().is_disabled());
    e.redo(Origin::User).unwrap();
    h.run_steps(4);
    assert_eq!(e.read(Clone::clone), after);
    click(&mut h, "Reload properties");
    click(&mut h, "Cancel properties");
}
#[test]
fn copper_row_via_precision_and_geometry_refusal_keep_local_fields_then_exact_apply() {
    let (e, mut h, _, vid) = setup(vec2(1600., 1000.));
    h.state_mut().begin_copper();
    h.run_steps(5);
    click(&mut h, &format!("Edit via {vid}"));
    let before = e.session_state();
    input(&mut h, "Property via diameter (mm)", "0.900003");
    input(&mut h, "Property via drill (mm)", "0.4000001");
    click(&mut h, "Apply properties");
    assert_eq!(e.session_state(), before);
    assert_eq!(h.get_by_label("Property via drill (mm)").accesskit_node().value(), Some("0.4000001"));
    input(&mut h, "Property via drill (mm)", "0.900003");
    click(&mut h, "Apply properties");
    assert_eq!(e.session_state(), before);
    input(&mut h, "Property via drill (mm)", "0.400001");
    click(&mut h, "Apply properties");
    let v = e.read(|p| p.board.vias[&vid].clone());
    assert_eq!(v.position, before.project.board.vias[&vid].position);
    assert_eq!(v.diameter, Nm(900003));
    assert_eq!(v.drill, Nm(400001));
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    assert!(
        h.state()
            .scene
            .items
            .iter()
            .any(|i| matches!(i.prim,Prim::Via{radius,drill_radius,..}if(radius-0.4500015).abs()<1e-12&&(drill_radius-0.2000005).abs()<1e-12))
    );
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}
#[test]
fn compact_controls_stay_in_view_cancel_escape_and_other_drafts_preserve_state() {
    let (e, mut h, tid, _) = setup(vec2(640., 400.));
    open(&e, &mut h, tid);
    let before = e.session_state();
    input(&mut h, "Property track width (mm)", "0.370001");
    for label in ["Apply runs scoped checks; excluded objects are not certified.", "Apply properties", "Reload properties", "Cancel properties"] {
        let r = h.get_by_label(label).rect();
        assert!(r.min.x >= 0. && r.min.y >= 0. && r.max.x <= 640. && r.max.y <= 400., "{label}: {r:?}");
    }
    h.state_mut().begin_route();
    h.state_mut().begin_copper();
    h.state_mut().begin_place_part();
    h.state_mut().begin_track_cleanup();
    h.state_mut().begin_named_net_rules();
    h.run_steps(3);
    assert!(!h.state().routing.previewing());
    assert!(h.query_by_label("Property track width (mm)").is_some());
    assert_eq!(e.session_state(), before);
    click(&mut h, "Cancel properties");
    assert!(h.query_by_label("Apply properties").is_none());
    assert_eq!(e.session_state(), before);
    h.state_mut().begin_copper_properties();
    h.run_steps(5);
    input(&mut h, "Property track width (mm)", "0.390001");
    key(&mut h, Key::Escape);
    assert!(h.query_by_label("Apply properties").is_none());
    assert_eq!(e.session_state(), before);
}
#[test]
fn peer_edit_and_same_uuid_reopen_disable_apply_preserve_fields_and_require_reload() {
    let (e, mut h, tid, _) = setup(vec2(1400., 1000.));
    open(&e, &mut h, tid);
    input(&mut h, "Property track width (mm)", "0.370001");
    let id = h.get_by_label("Property track width (mm)").accesskit_node().id();
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"peer","footprint":"x","x_mm":40,"y_mm":40}), Origin::agent("peer")).unwrap();
    h.run_steps(4);
    let peer = e.session_state();
    assert!(h.get_by_label("Apply properties").accesskit_node().is_disabled());
    click(&mut h, "Apply properties");
    assert_eq!(e.session_state(), peer);
    assert_eq!(h.get_by_label("Property track width (mm)").accesskit_node().value(), Some("0.370001"));
    assert_eq!(h.get_by_label("Property track width (mm)").accesskit_node().id(), id);
    click(&mut h, "Reload properties");
    assert_eq!(h.get_by_label("Property track width (mm)").accesskit_node().value(), Some("0.250000"));
    assert!(!h.get_by_label("Apply properties").accesskit_node().is_disabled());
    input(&mut h, "Property track width (mm)", "0.390001");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    h.run_steps(5);
    let reopened = e.session_state();
    assert_eq!(reopened.project.id, peer.project.id);
    assert_ne!(reopened.cursor.stream_id, peer.cursor.stream_id);
    assert!(h.get_by_label("Apply properties").accesskit_node().is_disabled());
    assert_eq!(h.get_by_label("Property track width (mm)").accesskit_node().value(), Some("0.390001"));
    click(&mut h, "Apply properties");
    assert_eq!(e.session_state(), reopened);
    click(&mut h, "Reload properties");
    input(&mut h, "Property track width (mm)", "0.410001");
    click(&mut h, "Apply properties");
    assert_eq!(e.read(|p| p.board.tracks[&tid].width), Nm(410001));
}
#[test]
fn mixed_or_empty_selection_opens_actionable_editor_without_mutation() {
    let (e, mut h, tid, vid) = setup(vec2(640., 400.));
    for ids in [vec![], vec![tid, vid]] {
        e.set_selection(SetSelection { ids, mode: Default::default() }, Origin::User).unwrap();
        let before = e.session_state();
        h.state_mut().begin_copper_properties();
        h.run_steps(5);
        assert!(h.get_by_label("Apply properties").accesskit_node().is_disabled());
        assert!(h.query_by_label("Select exactly one existing track or through via, or use its copper-row Edit button").is_some());
        click(&mut h, "Cancel properties");
        assert_eq!(e.session_state(), before);
    }
}
