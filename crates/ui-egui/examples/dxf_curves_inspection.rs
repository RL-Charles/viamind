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
    controls: Vec<&'static str>,
    notice: &'static str,
    verified: usize,
    requested: bool,
    ready_after: f64,
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
        // Collect current paint for every control label and the readable notice; require the
        // complete set fully inside the effective clip for three consecutive native passes.
        let painted = |prefix: &str, exact: bool| {
            shapes.iter().find_map(|s| {
                let egui::Shape::Text(t) = &s.shape else { return None };
                let text = t.galley.text();
                (if exact { text == prefix } else { text.starts_with(prefix) }).then_some((t.visual_bounding_rect(), s.clip_rect.intersect(content)))
            })
        };
        let mut bounds = Vec::new();
        let mut complete = true;
        for (label, exact) in self.controls.iter().map(|l| (*l, true)).chain(std::iter::once((self.notice, false))) {
            match painted(label, exact) {
                Some((r, c)) if content.contains_rect(r) && c.contains_rect(r) => bounds.push([r.min.x, r.min.y, r.max.x, r.max.y]),
                _ => complete = false,
            }
        }
        // Transferred egui memory keeps the prepared Harness clock's area fade-in timestamps;
        // count only frames after the native clock has passed them plus the fade duration.
        let settled = ui.input(|i| i.time) >= self.ready_after;
        self.verified = if complete && settled { self.verified + 1 } else { 0 };
        if self.ui_passes >= 6000 && self.verified == 0 {
            let texts: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().chars().take(40).collect::<String>()),
                    _ => None,
                })
                .collect();
            panic!(
                "controls/notice never fully visible; form open {}; preview retained {}; painted {texts:?}",
                self.app.outlines.editing(),
                self.app.outlines.dxf_preview().is_some()
            );
        }
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304);
            assert!(self.verified >= 3, "capture frame must keep every control and notice fully visible");
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
                serde_json::json!({"capture":"actual eframe native WGPU Screenshot event","pid":std::process::id(),"ui_passes":self.ui_passes,"path":path,"rgba_path":rawpath,"pixels":image.size,"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"os_input_claim":false,"control_and_notice_bounds":bounds,"consecutive_verified_passes":self.verified,"harness_clock_ready_after_s":self.ready_after,"preparation":"actual egui Harness inputs; native app/state/memory reuse; no OS input"})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.verified == 3 && !self.requested {
                self.requested = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 6100);
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
    let controls: Vec<&'static str> =
        if mode == "compact" { vec!["Preview curved DXF", "Apply curved DXF outline", "Cancel"] } else { vec!["Preview curved DXF", "Cancel"] };
    for label in &controls {
        reveal(&mut h, label);
    }
    h.get_by_label_contains(notice).scroll_to_me();
    h.run_steps(10);
    assert!(h.state().outlines.editing() && h.query_by_label("Board outline").is_some(), "prepared outline form must be open");
    let before = engine.session_state();
    let status = engine.document_status();
    let ready_after = h.ctx.input(|i| i.time) + f64::from(h.ctx.global_style().animation_time) + 0.25;
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
            Ok(Box::new(Inspect { app, engine, before, status, capture, ui_passes: 0, controls, notice, verified: 0, requested: false, ready_after }))
        }),
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(std::fs::read(&args[2])?, fixture);
    assert_eq!(std::fs::read(&args[3])?, dxf);
    Ok(())
}
