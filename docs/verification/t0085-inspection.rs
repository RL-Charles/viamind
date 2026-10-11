//! Temporary inspection harness: actual production controls and pointer input, then fixed native viewport.
use egui::{Event, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
// Inspection host keeps logical egui focus so compositor background focus loss
// does not cancel the already tested local draft. It sends no operating-system input.
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert!(!self.app.routing.pad_planned_points().is_empty());
    }
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.retain(|e| !matches!(e, Event::WindowFocused(false)));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("project path required")?;
    let mut fixture = viamind_doc::file::load(path)?;
    // Reconstruct a pre-route fixture before creating the real shared engine.
    fixture.board.tracks.retain(|_, t| t.net.as_deref() != Some("N"));
    let engine = viamind_engine::EngineHandle::new(fixture);
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_walkaround_route();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Start route").click();
    h.run_steps(5);
    for (x, y) in [(10., 10.), (30., 10.)] {
        let pos = h.state().canvas.camera.world_to_screen(WorldPos::new(x, y), h.state().canvas.viewport);
        for pressed in [true, false] {
            h.event(Event::PointerMoved(pos));
            h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
            h.run_steps(2);
        }
    }
    h.get_by_label("Plan static walkaround").click();
    h.run_steps(2);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().routing.pad_planned_points().is_empty() {
        h.run_steps(2);
        assert!(h.state().routing.error.is_none(), "{:?}", h.state().routing.error);
        assert!(std::time::Instant::now() < end, "plan timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(engine.session_state(), before);
    println!(
        "{}",
        serde_json::json!({"inspection_harness":true,"production_controls_and_pointer_events":true,"project_unchanged":true,"checked_points_nm":h.state().routing.pad_planned_points()})
    );
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind checked rectangular cutout inspection")
            .with_app_id("viamind-cutout-inspection")
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([1600., 1000.])
            .with_max_inner_size([1600., 1000.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind checked rectangular cutout inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspection { app, engine, before }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
