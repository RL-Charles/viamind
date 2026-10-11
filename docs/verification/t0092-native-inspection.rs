//! Read-only fixed viewport inspection; one documented local format-choice click.
use viamind_ui_egui::ViamindApp;
struct Inspection {
    inner: ViamindApp,
    frames: u32,
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.inner, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.inner, ui, frame);
        self.frames += 1;
        if self.frames < 50 {
            ui.ctx().request_repaint();
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        // Inspection-only click selects the local Catalog BOM option. This is
        // not a Cua input claim and does not modify the shared design engine.
        if matches!(self.frames, 25 | 26)
            && let Some(rect) = ctx.memory(|memory| memory.area_rect(egui::Id::new("Export assembly CSV")))
        {
            let pos = rect.min + egui::vec2(280.0, 68.0);
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos, button: egui::PointerButton::Primary, pressed: self.frames == 25,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("project path required")?;
    let engine = viamind_engine::EngineHandle::load(path)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind BOM template inspection")
            .with_app_id("viamind-bom-template-inspection")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1600.0, 1000.0])
            .with_max_inner_size([1600.0, 1000.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("Viamind BOM template inspection", options, Box::new(move |cc| {
        let mut inner = ViamindApp::with_design(engine, None);
        inner.configure_renderer(cc);
        inner.begin_assembly_export();
        Ok(Box::new(Inspection { inner, frames: 0 }))
    })).map_err(|error| error.to_string())?;
    Ok(())
}
