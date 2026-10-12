//! Inspection-only actual curved DXF form in a native eframe WGPU window; original channels retained.
//!
//! Preparation uses actual egui Harness inputs on the production app (no OS input), then
//! transfers the same app state and egui memory into a fixed-size native window, asserts
//! the full session is unchanged while painting, reads back one WGPU Screenshot and
//! closes itself.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;

struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    ui_passes: usize,
    controls: Vec<egui::Id>,
    notice: &'static str,
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
        let content = ui.ctx().content_rect();
        let shapes = ui.ctx().graphics(|g| g.clone().drain(&[], &Default::default()));
        assert!(shapes.len() <= 65536);
        let mut bounds = Vec::new();
        if self.ui_passes >= 3 {
            for id in &self.controls {
                let r = ui.ctx().read_response(*id).expect("same production widget identity after native memory transfer");
                assert!(content.contains_rect(r.rect));
                let clip = shapes
                    .iter()
                    .filter_map(|s| {
                        let egui::Shape::Rect(b) = &s.shape else { return None };
                        (b.rect.min.distance(r.rect.min) <= 0.5 && b.rect.max.distance(r.rect.max) <= 0.5).then_some(s.clip_rect.intersect(content))
                    })
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .expect("actual current native control paint");
                assert!(clip.contains_rect(r.rect), "actual native widget must fit effective clip");
                bounds.push([r.rect.min.x, r.rect.min.y, r.rect.max.x, r.rect.max.y]);
            }
            let (r, c) = shapes
                .iter()
                .find_map(|s| {
                    let egui::Shape::Text(t) = &s.shape else { return None };
                    t.galley.text().starts_with(self.notice).then_some((t.visual_bounding_rect(), s.clip_rect.intersect(content)))
                })
                .expect("current readable conversion notice");
            assert!(c.contains_rect(r), "notice must be fully visible");
            bounds.push([r.min.x, r.min.y, r.max.x, r.max.y]);
        }
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304);
            let path = self.capture.take().unwrap();
            let rawpath = path.with_extension("p7");
            let mut raw = std::fs::OpenOptions::new().write(true).create_new(true).open(&rawpath).unwrap();
            write!(raw, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1]).unwrap();
            raw.write_all(&image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b(), p.a()]).collect::<Vec<_>>()).unwrap();
            raw.sync_all().unwrap();
            let mut rgb = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            write!(rgb, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            rgb.write_all(&image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect::<Vec<_>>()).unwrap();
            rgb.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"actual eframe native WGPU Screenshot event","pid":std::process::id(),"ui_passes":self.ui_passes,"path":path,"rgba_path":rawpath,"pixels":image.size,"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"os_input_claim":false,"control_and_notice_bounds":bounds,"preparation":"actual egui Harness inputs; native app/state/memory reuse; no OS input"})
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
        raw.events.retain(|e| !matches!(e, Event::WindowFocused(false)));
    }
}
fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(10);
    let r = h.get_by_label(label).rect();
    h.hover_at(r.center());
    h.run_steps(2);
    let r = h.get_by_label(label).rect();
    let clip = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| {
            let matching = match &s.shape {
                egui::Shape::Text(t) => t.galley.text() == label,
                egui::Shape::Rect(b) => b.rect.min.distance(r.min) <= 0.5 && b.rect.max.distance(r.max) <= 0.5,
                _ => false,
            };
            matching.then_some(s.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("prepared current paint");
    assert!(clip.contains_rect(r), "{label} actual ancestor clip: response {r:?}, clip {clip:?}");
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    reveal(h, label);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn settle(h: &mut Harness<'_, ViamindApp>) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.state().outlines.dxf_busy() && std::time::Instant::now() < end {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!h.state().outlines.dxf_busy());
    h.run_steps(6);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 5, "compact|wide, native fixture, DXF source, NEW P6");
    let mode = args[1].as_str();
    assert!(matches!(mode, "compact" | "wide"));
    let fixture = std::fs::read(&args[2])?;
    let dxf = std::fs::read(&args[3])?;
    let engine = EngineHandle::new(viamind_doc::file::from_str(std::str::from_utf8(&fixture)?)?);
    let (width, height) = if mode == "compact" { (640.0, 400.0) } else { (1600.0, 1100.0) };
    let mut h = Harness::builder()
        .with_size(vec2(width, height))
        .build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(8);
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    click(&mut h, "Import ASCII DXF");
    input(&mut h, "DXF file path", &args[3]);
    input(&mut h, "DXF layer (empty = all)", "Edge");
    input(&mut h, "Curve tolerance (nm)", "10000");
    let initial = engine.session_state();
    click(&mut h, "Preview curved DXF");
    settle(&mut h);
    assert_eq!(engine.session_state(), initial, "preview never mutates");
    let notice = if mode == "wide" {
        click(&mut h, "Apply curved DXF outline");
        settle(&mut h);
        assert_ne!(engine.session_state().project.board.outline, initial.project.board.outline);
        h.get_by_label_contains("Applied curved DXF outline").scroll_to_me();
        "Applied curved DXF outline"
    } else {
        h.get_by_label_contains("Polygon approximation:").scroll_to_me();
        "Polygon approximation:"
    };
    h.run_steps(10);
    let mut controls = Vec::new();
    let labels: &[&str] =
        if mode == "compact" { &["Preview curved DXF", "Apply curved DXF outline", "Cancel"] } else { &["Preview curved DXF", "Cancel"] };
    for label in labels {
        reveal(&mut h, label);
        h.get_by_label(label).focus();
        h.run_steps(3);
        controls.push(h.ctx.memory(egui::Memory::focused).unwrap());
    }
    h.get_by_label_contains(notice).scroll_to_me();
    h.run_steps(10);
    let before = engine.session_state();
    let status = engine.document_status();
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind curved DXF inspection")
            .with_app_id("viamind-dxf-curve-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let capture = Some(PathBuf::from(&args[4]));
    eframe::run_native(
        "Viamind curved DXF inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture, ui_passes: 0, controls, notice }))
        }),
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(std::fs::read(&args[2])?, fixture);
    assert_eq!(std::fs::read(&args[3])?, dxf);
    Ok(())
}
