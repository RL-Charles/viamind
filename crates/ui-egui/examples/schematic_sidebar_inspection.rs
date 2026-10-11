//! Inspection-only self-closing native WGPU proof of production compact controls.
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;

struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    labels: Vec<&'static str>,
    capture: Option<PathBuf>,
    ui_passes: usize,
    stable_visible_passes: usize,
    screenshot_requested: bool,
    injected_wheel_events: usize,
}
fn control(shapes: &[egui::epaint::ClippedShape], label: &str, content: egui::Rect) -> Option<(egui::Rect, egui::Rect)> {
    let text = shapes.iter().find_map(|s| {
        let egui::Shape::Text(text) = &s.shape else { return None };
        (text.galley.job.text == label).then_some(text.visual_bounding_rect())
    })?;
    let (rect, clip) = shapes
        .iter()
        .filter_map(|s| {
            let egui::Shape::Rect(r) = &s.shape else { return None };
            (r.rect.contains_rect(text) && r.rect.width() <= text.width() + 24.0 && r.rect.height() <= text.height() + 24.0)
                .then_some((r.rect, s.clip_rect.intersect(content)))
        })
        .min_by(|a, b| a.0.area().total_cmp(&b.0.area()))?;
    clip.contains_rect(rect).then_some((rect, clip))
}

impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        if self.capture.is_none() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        self.ui_passes += 1;
        let shapes: Vec<_> = ui.ctx().graphics(|g| g.clone().drain(&[], &Default::default()));
        assert!(shapes.len() <= 32768);
        let content = ui.ctx().content_rect();
        let controls: Vec<_> = self.labels.iter().filter_map(|label| control(&shapes, label, content).map(|bounds| (label, bounds))).collect();
        if controls.len() == self.labels.len() {
            self.stable_visible_passes += 1;
        } else {
            self.stable_visible_passes = 0;
        }
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert_eq!(controls.len(), self.labels.len(), "every requested native control must still be fully visible at capture");
            assert!(self.stable_visible_passes >= 8);
            assert!(image.pixels.len() <= 4_194_304);
            let path = self.capture.take().unwrap();
            let rgba_path = path.with_extension("p7");
            let mut raw = std::fs::OpenOptions::new().write(true).create_new(true).open(&rgba_path).unwrap();
            write!(raw, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1]).unwrap();
            let rgba: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b(), p.a()]).collect();
            raw.write_all(&rgba).unwrap();
            raw.sync_all().unwrap();
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&rgb).unwrap();
            file.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"actual eframe native WGPU Screenshot event","pid":std::process::id(),"ui_passes":self.ui_passes,"os_input_claim":false,"preparation":"egui test preparation plus native raw_input_hook wheel events; no OS input","injected_wheel_events":self.injected_wheel_events,"path":path,"rgba_path":rgba_path,"pixels":image.size,"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"current_pass_controls":controls.iter().map(|(label,(r,c))|serde_json::json!({"label":label,"rect":[r.min.x,r.min.y,r.max.x,r.max.y],"effective_clip":[c.min.x,c.min.y,c.max.x,c.max.y]})).collect::<Vec<_>>()})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.stable_visible_passes >= 8 && !self.screenshot_requested {
                self.screenshot_requested = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 180);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.retain(|e| !matches!(e, egui::Event::WindowFocused(false)));
        if self.capture.is_some() && self.stable_visible_passes == 0 && self.ui_passes.is_multiple_of(4) {
            raw.events.push(egui::Event::PointerMoved(egui::pos2(100.0, 120.0)));
            raw.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -80.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            self.injected_wheel_events += 1;
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "entries|draft, native fixture, NEW P6 capture");
    assert!(matches!(args[1].as_str(), "entries" | "draft"));
    let path = PathBuf::from(&args[3]);
    assert!(!path.exists() && !path.with_extension("p7").exists());
    let source = std::fs::read(&args[2])?;
    let project = viamind_doc::file::from_str(std::str::from_utf8(&source)?)?;
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    let labels = if args[1] == "draft" {
        h.get_by_label("Add symbol").scroll_to_me();
        h.run_steps(10);
        h.get_by_label("Add symbol").click();
        h.run_steps(5);
        vec!["Apply schematic object", "Reload schematic draft", "Cancel schematic draft"]
    } else {
        vec!["Draw wires on canvas", "Annotate symbols"]
    };
    h.get_by_label(labels.last().unwrap()).scroll_to_me();
    h.run_steps(10);
    for label in &labels {
        let rect = h.get_by_label(label).rect();
        assert!(h.ctx.content_rect().contains_rect(rect));
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind compact schematic sidebar inspection")
            .with_app_id("viamind-sidebar-inspection")
            .with_inner_size([640.0, 400.0])
            .with_min_inner_size([640.0, 400.0])
            .with_max_inner_size([640.0, 400.0])
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind compact schematic sidebar inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect {
                app,
                engine,
                before,
                status,
                labels,
                capture: Some(path),
                ui_passes: 0,
                stable_visible_passes: 0,
                screenshot_requested: false,
                injected_wheel_events: 0,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(std::fs::read(&args[2])?, source);
    Ok(())
}
