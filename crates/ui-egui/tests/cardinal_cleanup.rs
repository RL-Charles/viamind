//! Actual egui selection, preview/apply and cancellation controls.
use egui::{Event, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Project, Track};
use viamind_engine::{EngineHandle, Origin};
use viamind_geom::{Nm, Point, Polygon};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};

fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 1_000_000), Nm(y * 1_000_000))
}
fn setup() -> (EngineHandle, Harness<'static, ViamindApp>) {
    let mut project = Project::new("Checked equivalent cleanup");
    project.board.outline = Some(BoardOutline::new(Polygon::new(vec![p(-30, -20), p(30, -20), p(30, 20), p(-30, 20)]), vec![]).unwrap());
    for (a, b) in [(-15, 0), (0, 15)] {
        let t = Track { id: ObjectId::new(), start: p(a, 0), end: p(b, 0), width: Nm(250_001), layer: CopperLayer::Top, net: Some("N".into()) };
        project.board.tracks.insert(t.id, t);
    }
    let e = EngineHandle::new(project);
    let mut h = Harness::builder()
        .with_size(vec2(1400., 1000.))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    (e, h)
}
fn click_track(h: &mut Harness<'_, ViamindApp>, x: f64, modifiers: Modifiers) {
    let pos = h.state().canvas.camera.world_to_screen(WorldPos::new(x, 0.), h.state().canvas.viewport);
    for pressed in [true, false] {
        h.event(Event::ModifiersChanged(modifiers));
        h.event(Event::PointerMoved(pos));
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers });
        h.run_steps(2);
    }
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().track_cleanup.checked().is_none() {
        h.run_steps(2);
        assert!(h.state().track_cleanup.message().is_none(), "{:?}", h.state().track_cleanup.message());
        assert!(std::time::Instant::now() < deadline, "cleanup worker timeout");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn actual_track_selection_checked_preview_atomic_apply_and_undo() {
    let (e, mut h) = setup();
    click_track(&mut h, -8., Modifiers::NONE);
    click_track(&mut h, 8., Modifiers::SHIFT);
    assert_eq!(e.selection().ids.len(), 2, "actual pointer selection must select both stored tracks");
    let before = e.session_state();
    h.state_mut().begin_track_cleanup();
    h.run_steps(3);
    h.get_by_label("Preview cleanup").click();
    h.run_steps(2);
    wait(&mut h);
    assert_eq!(h.state().track_cleanup.checked().unwrap().counts.deleted_tracks, 1);
    assert_eq!(e.session_state(), before);
    h.get_by_label("Apply cleanup").click();
    h.run_steps(3);
    assert!(!h.state().track_cleanup.editing());
    assert_eq!(e.read(|p| p.board.tracks.len()), 1);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 2, "one design and one pruned selection event");
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
}
#[test]
fn actual_net_source_cancel_peer_invalidation_and_modal_guards() {
    let (e, mut h) = setup();
    let before = e.session_state();
    h.state_mut().begin_track_cleanup();
    h.run_steps(3);
    h.get_by_label("Named net").click();
    h.run_steps(2);
    h.get_by_label("Top only").click();
    h.run_steps(2);
    h.get_by_label("Preview cleanup").click();
    h.run_steps(2);
    wait(&mut h);
    assert_eq!(e.session_state(), before);
    h.state_mut().begin_route();
    h.state_mut().begin_copper();
    h.state_mut().begin_place_part();
    assert!(h.state().track_cleanup.editing());
    assert!(!h.state().routing.previewing());
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"1k","footprint":"R","x_mm":25,"y_mm":15}), Origin::agent("peer")).unwrap();
    h.run_steps(3);
    assert!(h.state().track_cleanup.checked().is_none());
    let after_peer = e.session_state();
    h.get_by_label("Preview cleanup").click();
    h.run_steps(2);
    wait(&mut h);
    h.get_by_label("Cancel cleanup").click();
    h.run_steps(3);
    assert!(!h.state().track_cleanup.editing());
    assert_eq!(e.session_state(), after_peer);
}
