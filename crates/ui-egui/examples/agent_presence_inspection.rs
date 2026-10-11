//! Seeded transient attention rendered by the real native WGPU application.
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use viamind_engine::{EngineHandle, PresencePoint, SessionSnapshot, SetAgentPresence};
use viamind_ui_egui::ViamindApp;

struct Inspection {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    native: PathBuf,
    bytes: Vec<u8>,
    capture: PathBuf,
    report: PathBuf,
    started: Instant,
    passes: usize,
    captured: bool,
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(std::fs::read(&self.native).unwrap(), self.bytes);
        if self.captured {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        assert!(self.started.elapsed() < Duration::from_secs(20) && self.passes < 256, "bounded lease/capture deadline");
        assert_eq!(self.engine.query_agent_presence().agents.len(), 2);
        self.app.show(ui, None);
        self.passes += 1;
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(std::fs::read(&self.native).unwrap(), self.bytes);
        let image = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!((640..=1800).contains(&image.size[0]) && (400..=1200).contains(&image.size[1]));
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let mut file = std::fs::File::options().write(true).create_new(true).open(&self.capture).unwrap();
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&rgb).unwrap();
            file.sync_all().unwrap();
            let report = serde_json::json!({"inspection_only":true,"pid":std::process::id(),"capture":"actual eframe native WGPU Screenshot event","pixels":image.size,
                "ui_passes":self.passes,"agents":self.engine.query_agent_presence(),"full_session_unchanged":true,"native_bytes_unchanged":true,
                "seed_scope":"Two bounded direct trusted engine presence updates before native launch. Actual MCP ownership/stdio and human edit tests have separate evidence. No OS input or frame-cadence claim.","self_closes_after_fsynced_proof":true});
            let mut file = std::fs::File::options().write(true).create_new(true).open(&self.report).unwrap();
            serde_json::to_writer_pretty(&mut file, &report).unwrap();
            file.write_all(b"\n").unwrap();
            file.sync_all().unwrap();
            self.captured = true;
            println!("{report}");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let native = PathBuf::from(args.next().ok_or("native project required")?);
    let capture = PathBuf::from(args.next().ok_or("NEW capture required")?);
    let report = PathBuf::from(args.next().ok_or("NEW report required")?);
    let size = match args.next().as_deref().and_then(|s| s.to_str()) {
        None => [640., 400.],
        Some("wide") => [1600., 1000.],
        _ => return Err("optional fourth argument is wide".into()),
    };
    if args.next().is_some() || capture.try_exists()? || report.try_exists()? {
        return Err("NEW capture/report required".into());
    }
    let bytes = std::fs::read(&native)?;
    let engine = EngineHandle::load(&native)?;
    let before = engine.session_state();
    let parts: Vec<_> = before.project.board.parts.values().take(2).collect();
    if parts.len() != 2 {
        return Err("two PCB parts required for native inspection".into());
    }
    for (i, part) in parts.into_iter().enumerate() {
        engine.set_agent_presence(
            1001 + i as u64,
            ["Routing agent", "Assembly agent"][i],
            SetAgentPresence {
                surface: viamind_engine::PresenceSurface::Pcb {},
                expected_project: before.project.id,
                expected_stream: before.cursor.stream_id,
                cursor: PresencePoint { x: part.position.x, y: part.position.y },
                selection: vec![part.id],
                working_on: ["Checking via clearance", "Calibrating centroid"][i].into(),
            },
        )?;
    }
    let mut app = ViamindApp::with_design(engine.clone(), None);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind PCB agent presence inspection")
            .with_app_id("viamind-agent-presence-inspection")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind PCB agent presence inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspection { app, engine, before, native, bytes, capture, report, started: Instant::now(), passes: 0, captured: false }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
