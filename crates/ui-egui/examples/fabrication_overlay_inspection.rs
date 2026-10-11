//! Fixed native host for actual-input preparation of the production overlay UI.
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
        assert_eq!(self.engine.session_state(), self.before, "inspection must preserve complete session");
    }
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = std::env::args_os().nth(1).ok_or("project path required")?;
    let directory = std::path::PathBuf::from(std::env::args_os().nth(2).ok_or("fabrication directory required")?);
    let compact = std::env::args().nth(3).as_deref() == Some("compact");
    let (width, height) = if compact { (640.0, 400.0) } else { (1600.0, 1000.0) };
    let engine = EngineHandle::new(viamind_doc::file::load(project)?);
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_fabrication_inspection();
    let mut h = Harness::builder().with_size(egui::vec2(width, height)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    for (i, name) in
        ["top-copper.gbr", "bottom-copper.gbr", "top-mask.gbr", "bottom-mask.gbr", "profile.gbr", "plated-through.xnc"].into_iter().enumerate()
    {
        let label = format!("Fabrication file {} path", i + 1);
        h.get_by_label(&label).scroll_to_me();
        h.run_steps(3);
        h.get_by_label(&label).focus();
        h.run_steps(3);
        h.get_by_label(&label).type_text(directory.join(name).to_str().ok_or("UTF8 path required")?);
        h.run_steps(5);
    }
    click(&mut h, "Inspect selected fabrication files");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.query_by_label_contains("Inspected 6 files").is_none() {
        assert!(std::time::Instant::now() < deadline, "inspection failed or timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(3);
    }
    click(&mut h, "Show fabrication overlay");
    h.get_by_label("Show fabrication overlay").scroll_to_me();
    h.run_steps(8);
    let control = h.get_by_label("Show fabrication overlay").rect();
    assert!(control.min.x >= 0.0 && control.min.y >= 0.0 && control.max.x <= width && control.max.y <= height, "{control:?}");
    assert!(h.state().fabrication_view.painted_primitives() > 0);
    assert_eq!(engine.session_state(), before);
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind fabrication overlay inspection")
            .with_app_id("viamind-fabrication-overlay-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind fabrication overlay inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
