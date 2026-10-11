//! Headless frames of the whole app with `egui_kittest`.

use egui::{Event, Modifiers, MouseWheelUnit, TouchPhase, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use viamind_ui_egui::ViamindApp;

fn harness(objects: usize) -> Harness<'static, ViamindApp> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, app: &mut ViamindApp| app.show(ui, None),
            ViamindApp::with_scene_size(objects),
        )
}

#[test]
fn builds_frames_with_10k_scene() {
    let mut h = harness(10_000);
    h.run_steps(3);
    let app = h.state();
    assert_eq!(app.scene.items.len(), 10_000);
    // Zoomed to fit at start-up, so everything is on screen.
    assert_eq!(app.canvas.drawn, 10_000);
    assert!(!app.canvas.fit_pending);
    h.get_by_label("File");
    h.get_by_label("View");
    h.get_by_label("Layers");
}

#[test]
fn scroll_zooms_about_cursor_and_home_fits() {
    let mut h = harness(1_000);
    h.run_steps(2);
    let fit = h.state().canvas.camera;

    let cursor = pos2(700.0, 400.0);
    h.event(Event::PointerMoved(cursor));
    h.run_steps(1);
    let before = h
        .state()
        .canvas
        .cursor_world
        .expect("pointer over the canvas");
    h.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 200.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    // Scrolling is smoothed over several frames.
    h.run_steps(30);
    let cam = h.state().canvas.camera;
    assert!(cam.zoom > fit.zoom * 1.2, "{} vs {}", cam.zoom, fit.zoom);
    let after = h
        .state()
        .canvas
        .cursor_world
        .expect("pointer over the canvas");
    assert!((after.x - before.x).abs() < 1e-3 && (after.y - before.y).abs() < 1e-3);
    assert!(h.state().canvas.drawn < 1_000);

    h.key_press(egui::Key::Home);
    h.run_steps(1);
    assert_eq!(h.state().canvas.camera, fit);
}

#[test]
fn view_menu_switches_scene_size() {
    let mut h = harness(1_000);
    h.run_steps(1);
    h.get_by_label("View").click();
    h.run_steps(1);
    h.get_by_label("100k objects").click();
    h.run_steps(2);
    assert_eq!(h.state().scene.items.len(), 100_000);
    assert_eq!(h.state().canvas.drawn, 100_000);
}
