use viamind_ui_egui::ViamindApp;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("project path required")?;
    let settings = std::env::args().any(|arg| arg == "--settings");
    let engine = viamind_engine::EngineHandle::new(viamind_doc::file::load(path)?);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind single through-via routing inspection")
            .with_app_id("viamind-through-via-routing-inspection")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1600.0, 1000.0])
            .with_max_inner_size([1600.0, 1000.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind single through-via routing inspection",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_design(engine, None);
            app.configure_renderer(cc);
            if settings { app.begin_via_route(); }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
