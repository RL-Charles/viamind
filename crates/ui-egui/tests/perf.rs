//! Frame-time measurement for the 100k-object scene (CPU side: ui build plus tessellation).
//!
//! Ignored by default because debug builds are not representative. Run with:
//! `cargo test --release -p viamind-ui-egui --test perf -- --ignored --nocapture`

use std::time::{Duration, Instant};

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use viamind_ui_egui::ViamindApp;

const FRAMES: usize = 60;

struct Timing {
    ui: Duration,
    tessellate: Duration,
}

fn input(screen: Rect, events: Vec<Event>, time: f64) -> RawInput {
    RawInput { screen_rect: Some(screen), time: Some(time), events, ..Default::default() }
}

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE }
}

/// Runs `FRAMES` frames of a middle-drag pan back and forth over the canvas at the zoom given by
/// `zoom_factor` relative to zoom-to-fit, and returns per-frame timings.
fn measure(objects: usize, zoom_factor: f64) -> (Vec<Timing>, usize) {
    let ctx = egui::Context::default();
    let mut app = ViamindApp::with_scene_size(objects);
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0));
    let center = pos2(900.0, 500.0);
    let mut t = 0.0;

    // Warm up: lay out panels, zoom to fit, then apply the requested zoom.
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run_ui(input(screen, vec![], t), |ui| app.show(ui, None));
    }
    let canvas = Rect::from_min_max(pos2(200.0, 30.0), pos2(1600.0, 970.0));
    app.canvas.camera.zoom_about(center, canvas, zoom_factor);
    t += 1.0 / 60.0;
    let start = vec![Event::PointerMoved(center), button(center, PointerButton::Middle, true)];
    let _ = ctx.run_ui(input(screen, start, t), |ui| app.show(ui, None));

    let mut timings = Vec::with_capacity(FRAMES);
    let mut pos = center;
    for i in 0..FRAMES {
        t += 1.0 / 60.0;
        let dx = if (i / 15) % 2 == 0 { 4.0 } else { -4.0 };
        pos += vec2(dx, dx * 0.5);
        let t0 = Instant::now();
        let out = ctx.run_ui(input(screen, vec![Event::PointerMoved(pos)], t), |ui| app.show(ui, None));
        let t1 = Instant::now();
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        let t2 = Instant::now();
        assert!(!prims.is_empty());
        timings.push(Timing { ui: t1 - t0, tessellate: t2 - t1 });
    }
    (timings, app.canvas.drawn)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn report(label: &str, objects: usize, zoom_factor: f64) -> f64 {
    let (timings, drawn) = measure(objects, zoom_factor);
    let mut total: Vec<f64> = timings.iter().map(|t| ms(t.ui + t.tessellate)).collect();
    total.sort_by(f64::total_cmp);
    let median = total[total.len() / 2];
    let p95 = total[total.len() * 95 / 100];
    let ui_mean = timings.iter().map(|t| ms(t.ui)).sum::<f64>() / timings.len() as f64;
    let tess_mean = timings.iter().map(|t| ms(t.tessellate)).sum::<f64>() / timings.len() as f64;
    println!(
        "{label}: {drawn}/{objects} drawn, frame median {median:.2} ms, p95 {p95:.2} ms \
         (ui {ui_mean:.2} ms + tessellate {tess_mean:.2} ms)"
    );
    median
}

#[test]
#[ignore = "timing; run in release with --ignored --nocapture"]
fn frame_time_100k_scene() {
    let fit = report("100k fit (all visible)", 100_000, 1.0);
    report("100k zoomed 8x (culled)", 100_000, 8.0);
    report("10k fit", 10_000, 1.0);
    println!("100k scene frame ms = {fit:.2}");
}
