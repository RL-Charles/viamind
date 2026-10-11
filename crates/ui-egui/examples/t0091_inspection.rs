//! Retained inspection host: production cleanup controls with an immutable native preview.
use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.app.track_cleanup.checked().expect("retained checked preview").counts.deleted_tracks, 2);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("project path required")?;
    let fixture = viamind_doc::file::load(path)?;
    let engine = viamind_engine::EngineHandle::new(fixture);
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_track_cleanup();
    let mut h = Harness::builder().with_size(vec2(1600., 1000.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Named net").click();
    h.run_steps(3);
    h.get_by_label("Preview cleanup").click();
    h.run_steps(2);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().track_cleanup.checked().is_none() {
        h.run_steps(2);
        assert!(h.state().track_cleanup.message().is_none(), "{:?}", h.state().track_cleanup.message());
        assert!(std::time::Instant::now() < end, "plan timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(h.state().track_cleanup.checked().unwrap().counts.deleted_tracks, 2);
    let mut controls = serde_json::Map::new();
    for label in ["Preview cleanup", "Apply cleanup", "Cancel cleanup"] {
        let rect = h.get_by_label(label).rect();
        assert!(rect.min.x >= 0. && rect.min.y >= 0. && rect.max.x <= 1600. && rect.max.y <= 1000., "{label} outside native viewport: {rect:?}");
        controls.insert(label.into(), serde_json::json!({"min":[rect.min.x,rect.min.y],"max":[rect.max.x,rect.max.y]}));
    }
    println!(
        "{}",
        serde_json::json!({"inspection_harness":true,"production_controls_via_egui_events":true,"project_unchanged":true,"checked":h.state().track_cleanup.checked(),"control_bounds":controls})
    );
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind checked cardinal cleanup inspection")
            .with_app_id("viamind-cardinal-cleanup-inspection")
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([1600., 1000.])
            .with_max_inner_size([1600., 1000.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind checked cardinal cleanup inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspection { app, engine, before }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
