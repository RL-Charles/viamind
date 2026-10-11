//! Actual native WGPU status inspection after local egui human inputs; closes after fsynced proof.
use egui_kittest::{Harness, kittest::Queryable};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use viamind_engine::{EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;

struct Inspection {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    native: PathBuf,
    native_bytes: Vec<u8>,
    capture: PathBuf,
    report: PathBuf,
    ui_passes: usize,
    captured: bool,
    started: Instant,
    server: viamind_mcp::HttpServer,
}
impl Drop for Inspection {
    fn drop(&mut self) {
        self.server.shutdown();
    }
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.));
        self.ui_passes += 1;
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(std::fs::read(&self.native).unwrap(), self.native_bytes);
        if self.captured {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        assert!(self.started.elapsed() < Duration::from_secs(120) && self.ui_passes < 256, "bounded capture deadline");
        let screenshot = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = screenshot {
            assert!(image.size[0] >= 640 && image.size[0] <= 2560 && image.size[1] >= 400 && image.size[1] <= 1600);
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let mut file = std::fs::File::options().write(true).create_new(true).open(&self.capture).unwrap();
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&rgb).unwrap();
            file.sync_all().unwrap();
            let report = serde_json::json!({"inspection_only":true,"mcp_enabled":true,"agent_read_only":true,"pid":std::process::id(),
                "url":self.server.url(),"capture":"actual eframe native WGPU Screenshot event","pixels":image.size,
                "ui_passes":self.ui_passes,"full_session_unchanged_during_native_ui":true,"native_bytes_preserved":true,
                "human_local_egui_actions_verified":["Edit menu","Place test resistor","Ctrl+Z","Ctrl+Shift+Z"],
                "input_scope":"local egui test input before native rendering; no OS input or frame-cadence claim",
                "self_closes_after_fsynced_proof":true});
            let mut file = std::fs::File::options().write(true).create_new(true).open(&self.report).unwrap();
            serde_json::to_writer_pretty(&mut file, &report).unwrap();
            file.write_all(b"\n").unwrap();
            file.sync_all().unwrap();
            self.captured = true;
            println!("{report}");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}
fn keyboard(h: &mut Harness<'_, ViamindApp>, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed, repeat: false, modifiers });
        h.run_steps(4);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let native = PathBuf::from(args.next().ok_or("native project required")?);
    let capture = PathBuf::from(args.next().ok_or("NEW capture required")?);
    let report = PathBuf::from(args.next().ok_or("NEW report required")?);
    if args.next().is_some() || capture.try_exists()? || report.try_exists()? {
        return Err("three paths; both outputs must be NEW".into());
    }
    let native_bytes = std::fs::read(&native)?;
    let engine = EngineHandle::load(&native)?;
    let initial = engine.session_state();
    let server = viamind_mcp::spawn_engine_server_with_mode(engine.clone(), "127.0.0.1:0".parse()?, true)?;
    let app = ViamindApp::with_design_agent_mode(engine.clone(), Some(server.url()), true);
    let mut h = Harness::builder().with_size(egui::vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Agents read only");
    h.get_by_label("Edit").click();
    h.run_steps(4);
    h.get_by_label("Place test resistor").click();
    h.run_steps(5);
    assert_eq!(engine.read(|p| p.board.parts.len()), initial.project.board.parts.len() + 1);
    let edited = engine.read(Clone::clone);
    keyboard(&mut h, egui::Modifiers::COMMAND);
    assert_eq!(engine.read(Clone::clone), initial.project);
    keyboard(&mut h, egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT));
    assert_eq!(engine.read(Clone::clone), edited);
    assert_eq!(std::fs::read(&native)?, native_bytes);
    let before = engine.session_state();
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind agent read-only native inspection")
            .with_app_id("viamind-agent-read-only-inspection")
            .with_inner_size([640., 400.])
            .with_min_inner_size([640., 400.])
            .with_max_inner_size([640., 400.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind agent read-only native inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspection {
                app,
                engine,
                before,
                native,
                native_bytes,
                capture,
                report,
                ui_passes: 0,
                captured: false,
                started: Instant::now(),
                server,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
