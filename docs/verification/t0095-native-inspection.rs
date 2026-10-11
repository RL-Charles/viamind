//! Inspection-only local input and NEW native WGPU capture. No MCP listener.
use std::{io::Write, path::PathBuf};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    target: Option<egui::Pos2>,
    inputs: u32,
    injected: u32,
    requested: bool,
    capture: Option<PathBuf>,
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.app, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
        assert_eq!(self.engine.session_state(), self.before);
        let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Saved output jobs")));
        let (target, checked_visible) = ui.ctx().graphics(|graphics| {
            let mut target = None;
            let mut checked = false;
            if let Some(list) = graphics.get(layer) {
                for entry in list.all_entries() {
                    if let egui::Shape::Text(text) = &entry.shape {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        if entry.clip_rect.intersects(rect) {
                            if text.galley.text() == "Checked outer 2-layer" { target = Some(rect.center()); }
                            if text.galley.text() == "Copper clearance baseline (mm)" { checked = true; }
                        }
                    }
                }
            }
            (target, checked)
        });
        self.target = target;
        if let Some(path) = &self.capture {
            let screenshot = ui.input(|input| input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()), _ => None,
            }));
            if let Some(image) = screenshot {
                assert!(checked_visible && self.injected == 2);
                assert!(image.size[0].saturating_mul(image.size[1]) <= 4_000_000);
                assert!(image.pixels.iter().all(|pixel| pixel.a() == 255));
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).expect("NEW capture path");
                write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).expect("PPM header");
                let rgb: Vec<_> = image.pixels.iter().flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()]).collect();
                file.write_all(&rgb).expect("RGB pixels");
                file.sync_all().expect("capture sync");
                println!("{}", serde_json::json!({"capture":"eframe native WGPU Screenshot event","pixels":image.size,"path":path,"pid":std::process::id(),"executable":std::env::current_exe().unwrap(),"injected_profile_pointer_events":self.injected,"checked_fields_visible":checked_visible,"full_session_unchanged":true,"source_cursor":self.before.cursor}));
                std::io::stdout().flush().expect("report flush");
                self.capture = None;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.inputs >= 30 && checked_visible && !self.requested {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.requested = true;
            }
        }
        if self.capture.is_some() { ui.ctx().request_repaint(); }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        self.inputs += 1;
        // Paint-derived label bounds avoid guessing window coordinates. This is
        // local egui input, not Cua/global input, and changes no shared design.
        if matches!(self.inputs, 25 | 26) && let Some(pos) = self.target {
            self.injected += 1;
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: self.inputs == 25, modifiers: egui::Modifiers::NONE });
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project = args.next().ok_or("project required")?;
    let capture = PathBuf::from(args.next().ok_or("NEW PPM capture path required")?);
    let engine = viamind_engine::EngineHandle::load(project)?;
    let before = engine.session_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Viamind checked job inspection").with_app_id("viamind-checked-job-inspection")
            .with_inner_size([1600.,1000.]).with_min_inner_size([1600.,1000.]).with_max_inner_size([1600.,1000.]).with_resizable(false),
        renderer: eframe::Renderer::Wgpu, ..Default::default()
    };
    eframe::run_native("Viamind checked job inspection", options, Box::new(move |cc| {
        let mut app = ViamindApp::with_design(engine.clone(), None); app.configure_renderer(cc); app.begin_output_jobs();
        Ok(Box::new(Inspection { app, engine, before, target:None, inputs:0, injected:0, requested:false, capture:Some(capture) }))
    })).map_err(|error| error.to_string())?;
    Ok(())
}
