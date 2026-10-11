//! Inspection-only production schematic canvas with self-closing native WGPU readback.
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_render::schematic_labels::{SymbolLabel, label_union, schematic_label_layout};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    plan: Vec<SymbolLabel>,
    capture: Option<PathBuf>,
    ui_passes: usize,
    low: bool,
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
        let cam = self.app.schematic.camera;
        let viewport = self.app.schematic.viewport;
        let screen = |x: i64, y: i64| cam.world_to_screen(WorldPos::new(x as f64 / 1e6, y as f64 / 1e6), viewport);
        let mut measured = vec![];
        if self.ui_passes >= 3 {
            assert!(ui.ctx().content_rect().contains_rect(viewport));
            if self.low {
                assert!(cam.zoom < 10.0);
            } else {
                assert!(cam.zoom >= 10.0);
                assert!(self.plan.len() <= 100);
                for label in &self.plan {
                    let b = label.envelope;
                    let envelope = egui::Rect::from_min_max(screen(b.min_x, b.min_y), screen(b.max_x, b.max_y));
                    assert!(viewport.contains_rect(envelope), "native full label envelope visible {}: {envelope:?} {viewport:?}", label.id);
                    let at = screen(label.anchor.x.0, label.anchor.y.0);
                    let galley = ui.ctx().fonts_mut(|f| {
                        f.layout_no_wrap(label.display_text.clone(), egui::FontId::monospace(cam.zoom.min(32.0) as f32), egui::Color32::WHITE)
                    });
                    let rect = galley.rect.translate(at.to_vec2());
                    let ink = galley.mesh_bounds.translate(at.to_vec2());
                    assert!(envelope.contains_rect(rect));
                    assert!(!ink.is_positive() || envelope.contains_rect(ink));
                    for other in &measured {
                        assert!(!rect.intersects(*other), "native actual text rectangles disjoint");
                    }
                    measured.push(rect);
                }
            }
        }
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 16_777_216);
            let path = self.capture.take().unwrap();
            let rgba_path = path.with_extension("p7");
            let mut raw = std::fs::OpenOptions::new().write(true).create_new(true).open(&rgba_path).unwrap();
            write!(raw, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1]).unwrap();
            let rgba: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b(), p.a()]).collect();
            raw.write_all(&rgba).unwrap();
            raw.sync_all().unwrap();
            let mut rgb_file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            write!(rgb_file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            rgb_file.write_all(&rgb).unwrap();
            rgb_file.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"actual eframe native WGPU Screenshot event","os_input_claim":false,"pid":std::process::id(),"ui_passes":self.ui_passes,"path":path,"rgba_path":rgba_path,"pixels":image.size,"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"nonopaque_alpha_pixels":image.pixels.iter().filter(|p|p.a()!=255).count(),"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"low_zoom_labels_suppressed":self.low,"camera":{"x_mm":cam.center.x,"y_mm":cam.center.y,"pixels_per_mm":cam.zoom},"viewport":[viewport.min.x,viewport.min.y,viewport.max.x,viewport.max.y],"actual_text_bounds":measured.iter().map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(),"shared_plan":self.plan})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 180);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.retain(|e| !matches!(e, egui::Event::WindowFocused(false)));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "mode wide|compact|low, native fixture, NEW P6 capture");
    let mode = &args[1];
    assert!(matches!(mode.as_str(), "wide" | "compact" | "low"));
    let capture = PathBuf::from(&args[3]);
    assert!(!capture.exists() && !capture.with_extension("p7").exists());
    let bytes = std::fs::read(&args[2])?;
    let project = viamind_doc::file::from_str(std::str::from_utf8(&bytes)?)?;
    let sheet = project.schematic.sheets.values().next().unwrap();
    let plan = schematic_label_layout(sheet)?;
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let size = if mode == "wide" { [1800.0, 1200.0] } else { [640.0, 400.0] };
    let mut h = Harness::builder().with_size(egui::vec2(size[0], size[1])).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Fit schematic").click();
    h.run_steps(5);
    if mode != "wide" {
        let b = plan.iter().map(|l| l.envelope).reduce(label_union).unwrap();
        h.state_mut().schematic.camera.center = WorldPos::new((b.min_x as f64 + b.max_x as f64) / 2e6, (b.min_y as f64 + b.max_y as f64) / 2e6);
        h.state_mut().schematic.camera.zoom = if mode == "low" { 5.0 } else { 10.0 };
        h.run_steps(3);
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read(&args[2])?, bytes);
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let low = mode == "low";
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind original schematic label inspection")
            .with_app_id("viamind-label-inspection")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind original schematic label inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, plan, capture: Some(capture), ui_passes: 0, low }))
        }),
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(std::fs::read(&args[2])?, bytes);
    Ok(())
}
