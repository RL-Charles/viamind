//! Read-only fixed viewport inspection; one documented local format-choice click.
use viamind_ui_egui::ViamindApp;
use std::{io::Write, path::PathBuf};
struct Inspection {
    inner: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    frames: u32,
    input_frames: u32,
    injected: u32,
    capture: Option<PathBuf>,
    capture_requested: bool,
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.inner, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.inner, ui, frame);
        assert_eq!(self.engine.session_state(), self.before);
        self.frames += 1;
        if let Some(path) = &self.capture {
            let screenshot = ui.input(|input| input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            }));
            if let Some(image) = screenshot {
                assert!(image.size[0].saturating_mul(image.size[1]) <= 4_000_000);
                assert!(image.pixels.iter().all(|pixel| pixel.a() == 255));
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).expect("NEW capture path");
                write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).expect("PPM header");
                let pixels: Vec<_> = image.pixels.iter().flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()]).collect();
                file.write_all(&pixels).expect("opaque native framebuffer pixels");
                println!("{}", serde_json::json!({"capture":"eframe native WGPU Screenshot event","pixels":image.size,"path":path,"injected_format_pointer_events":self.injected,"full_session_unchanged":true,"source_cursor":self.before.cursor}));
                self.capture = None;
            } else if self.input_frames >= 30 && !self.capture_requested {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.capture_requested = true;
            }
        }
        if self.input_frames < 35 || self.capture.is_some() {
            ui.ctx().request_repaint();
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        // Inspection-only click selects the local Catalog BOM option. This is
        // not a Cua input claim and does not modify the shared design engine.
        self.input_frames += 1;
        if matches!(self.input_frames, 25 | 26)
            // Window derives its area id from Atoms::text(), an Option<Cow<str>>.
            && let Some(rect) = ctx.memory(|memory| memory.area_rect(egui::Id::new(Some("Export assembly CSV"))))
        {
            self.injected += 1;
            let pos = rect.min + egui::vec2(280.0, 68.0);
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos, button: egui::PointerButton::Primary, pressed: self.input_frames == 25,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("project path required")?;
    let capture = args.next().map(PathBuf::from);
    let engine = viamind_engine::EngineHandle::load(path)?;
    let before = engine.session_state();
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
        let mut inner = ViamindApp::with_design(engine.clone(), None);
        inner.configure_renderer(cc);
        inner.begin_assembly_export();
        Ok(Box::new(Inspection { inner, engine, before, frames: 0, input_frames: 0, injected: 0, capture, capture_requested: false }))
    })).map_err(|error| error.to_string())?;
    Ok(())
}
