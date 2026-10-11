//! Inspection-only native rendering host; the actual AppImage is verified separately.
use std::{io::Write, path::PathBuf, time::Duration};
use viamind_ui_egui::ViamindApp;

struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    capture: Option<PathBuf>,
    ui_passes: usize,
}

impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before, "inspection must preserve the complete session");
        self.ui_passes += 1;
        let Some(path) = &self.capture else { return };
        let screenshot = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = screenshot {
            assert!(image.size[0] >= 1600 && image.size[1] >= 1000);
            assert!(image.size[0] <= 6400 && image.size[1] <= 4000);
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let mut file = std::fs::File::options().write(true).create_new(true).open(path).expect("NEW native capture");
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).expect("PPM header");
            let pixels: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&pixels).expect("native RGB framebuffer");
            file.sync_all().expect("capture durability");
            let rect = ui.ctx().content_rect();
            println!(
                "{}",
                serde_json::json!({
                    "inspection_only":true, "mcp_enabled":false,
                    "capture":"eframe native WGPU framebuffer Screenshot event",
                    "path":path, "pixels":image.size, "ui_passes":self.ui_passes,
                    "full_session_unchanged":true,
                    "content_rect":[rect.min.x,rect.min.y,rect.max.x,rect.max.y]
                })
            );
            self.capture = None;
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let fixture = viamind_doc::file::load(args.next().ok_or("project required")?)?;
    let capture = PathBuf::from(args.next().ok_or("NEW capture path required")?);
    if args.next().is_some() || capture.try_exists()? {
        return Err("exactly a project and NEW capture path are required".into());
    }
    let engine = viamind_engine::EngineHandle::new(fixture);
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind Linux packaging rendering inspection")
            .with_app_id("viamind-appimage-rendering-inspection")
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([1600., 1000.])
            .with_max_inner_size([1600., 1000.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind Linux packaging rendering inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspection { app, engine, before, capture: Some(capture), ui_passes: 0 }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
