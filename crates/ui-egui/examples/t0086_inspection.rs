//! Readable inspection of the production ViamindApp prepared through actual egui input.
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before, "inspection must preserve the complete session");
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = std::env::args_os().nth(1).ok_or("project path required")?;
    let library = std::env::args().nth(2).ok_or("library path required")?;
    let engine = viamind_engine::EngineHandle::new(viamind_doc::file::load(project)?);
    let initial = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_symbol_library();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.query_by_label_contains("Target: Power regulator").is_none() {
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
    h.get_by_label("Symbol library file path").focus();
    h.run_steps(3);
    h.get_by_label("Symbol library file path").type_text(&library);
    h.run_steps(5);
    h.get_by_label("Load symbol template").click();
    h.run_steps(5);
    while h.query_by_label_contains("Loaded Original regulator").is_none() {
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
    h.get_by_label("Edit loaded definition").click();
    h.run_steps(5);
    let target = if std::env::args().nth(3).as_deref() == Some("save") { "Save NEW definition file" } else { "Definition height (mm)" };
    h.get_by_label(target).scroll_to_me();
    h.run_steps(8);
    assert_eq!(engine.session_state(), initial, "inspection preparation is read-only");
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind native symbol definition inspection")
            .with_app_id("viamind-symbol-definition-inspection")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1600.0, 1000.0])
            .with_max_inner_size([1600.0, 1000.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind native symbol definition inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before: initial }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
