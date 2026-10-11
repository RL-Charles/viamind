//! Inspection-only host: production exact-net editor, actual native WGPU readback.
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    capture: PathBuf,
    ui_passes: usize,
    requested: bool,
    captured: bool,
    bounds: Vec<egui::Rect>,
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.captured {
            assert_eq!(self.engine.session_state(), self.before);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.));
        self.ui_passes += 1;
        assert_eq!(self.engine.session_state(), self.before);
        assert!(self.app.net_rules.editing());
        let content = ui.ctx().content_rect();
        assert!(self.bounds.iter().all(|r| content.contains_rect(*r)), "prepared actual egui controls must fit native viewport");
        let screenshot =
            ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(image) = screenshot {
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let mut file = std::io::BufWriter::new(
                std::fs::File::options().create_new(true).write(true).open(&self.capture).expect("exclusive new capture path"),
            );
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            for pixel in &image.pixels {
                file.write_all(&[pixel.r(), pixel.g(), pixel.b()]).unwrap();
            }
            file.flush().unwrap();
            file.get_ref().sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"actual eframe native WGPU Screenshot event","path":self.capture,"pixels":image.size,"ui_passes":self.ui_passes,"full_session_unchanged":true,"prepared_actual_egui_control_bounds_fit_native_viewport":true,"self_close_after_fsync":true,"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
            );
            std::io::stdout().flush().unwrap();
            self.captured = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes >= 8 && !self.requested {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.requested = true;
            }
            assert!(self.ui_passes < 1000, "bounded capture wait expired");
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("fixture path required")?;
    let capture = PathBuf::from(args.next().ok_or("new capture path required")?);
    let compact = args.next().is_some();
    let size = if compact { [640., 400.] } else { [1600., 1000.] };
    let engine = viamind_engine::EngineHandle::new(viamind_doc::file::load(path)?);
    let before = engine.session_state();
    assert!(!before.project.board.net_rules.is_empty());
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_named_net_rules();
    let mut h = Harness::builder().with_size(egui::vec2(size[0], size[1])).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(6);
    let mut bounds = Vec::new();
    let mut controls = serde_json::Map::new();
    for label in ["Apply named-net rules", "Reload rules", "Cancel rules"] {
        let rect = h.get_by_label(label).rect();
        assert!(rect.min.x >= 0. && rect.min.y >= 0. && rect.max.x <= size[0] && rect.max.y <= size[1]);
        bounds.push(rect);
        controls.insert(label.into(), serde_json::json!({"min":[rect.min.x,rect.min.y],"max":[rect.max.x,rect.max.y]}));
    }
    assert_eq!(engine.session_state(), before);
    println!(
        "{}",
        serde_json::json!({"inspection_only":true,"prepared_production_editor_via_egui":true,"os_input_claim":false,"full_session_unchanged":true,"control_bounds":controls,"named_rules":before.project.board.net_rules})
    );
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind exact named-net minima inspection")
            .with_app_id("viamind-net-minima-inspection")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind exact named-net minima inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspection { app, engine, before, capture, ui_passes: 0, requested: false, captured: false, bounds }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
