//! Inspection-only production bus preview prepared with local egui inputs; original native channels retained.
use egui::{Event, Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    native: Vec<u8>,
    path: PathBuf,
    output: PathBuf,
    size: [usize; 2],
    passes: usize,
    controls: Vec<egui::Id>,
    prepared: serde_json::Value,
    time_offset: f64,
    finished: bool,
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.finished {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        self.passes += 1;
        assert!(self.passes < 180, "bounded native bus capture");
        let content = ui.ctx().content_rect();
        let shapes = ui.ctx().graphics(|g| g.clone().drain(&[], &Default::default()));
        assert!(shapes.len() <= 32768);
        let mut bounds = Vec::new();
        if self.passes >= 6 {
            for id in &self.controls {
                let r = ui.ctx().read_response(*id).expect("same production footer widget identity after memory transfer");
                assert!(r.enabled());
                let clip = shapes
                    .iter()
                    .filter_map(|s| {
                        let egui::Shape::Rect(b) = &s.shape else {
                            return None;
                        };
                        ((b.rect.min - r.rect.min).abs().max_elem() <= 0.5 && (b.rect.max - r.rect.max).abs().max_elem() <= 0.5)
                            .then_some(s.clip_rect.intersect(content))
                    })
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .unwrap_or_else(|| {
                        let near: Vec<_> = shapes
                            .iter()
                            .filter_map(|s| match &s.shape {
                                egui::Shape::Rect(b) if b.rect.expand(4.0).intersects(r.rect) => Some((b.rect, s.clip_rect)),
                                _ => None,
                            })
                            .collect();
                        let texts: Vec<_> = shapes
                            .iter()
                            .filter_map(|s| match &s.shape {
                                egui::Shape::Text(t) if t.visual_bounding_rect().expand(24.0).intersects(r.rect) => {
                                    Some((t.galley.text().to_owned(), t.visual_bounding_rect()))
                                }
                                _ => None,
                            })
                            .collect();
                        panic!(
                            "exact native painted button background for {:?} at pass {}; nearby rects {near:?}; texts {texts:?}",
                            r.rect, self.passes
                        )
                    });
                assert!(clip.contains_rect(r.rect), "footer response must fit actual painted ancestor clip");
                bounds.push(serde_json::json!({"response":[r.rect.min.x,r.rect.min.y,r.rect.max.x,r.rect.max.y],"effective_clip":[clip.min.x,clip.min.y,clip.max.x,clip.max.y]}));
            }
            let result = shapes
                .iter()
                .find_map(|s| {
                    let egui::Shape::Text(t) = &s.shape else {
                        return None;
                    };
                    t.galley.text().starts_with("Preview: ").then_some((t.visual_bounding_rect(), s.clip_rect.intersect(content)))
                })
                .expect("actual structured preview summary paint");
            assert!(result.1.contains_rect(result.0));
        }
        let image = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert_eq!(image.size, self.size);
            assert_eq!(std::fs::read(&self.path).unwrap(), self.native);
            let rgba: Vec<_> = image.pixels.iter().flat_map(egui::Color32::to_array).collect();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            let pam = format!("P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", self.size[0], self.size[1]);
            let ppm = format!("P6\n{} {}\n255\n", self.size[0], self.size[1]);
            for (name, header, bytes) in [("native.p7", pam.as_bytes(), rgba.as_slice()), ("native.p6", ppm.as_bytes(), rgb.as_slice())] {
                let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join(name)).unwrap();
                f.write_all(header).unwrap();
                f.write_all(bytes).unwrap();
                f.sync_all().unwrap();
            }
            let report = serde_json::json!({"passed":true,"route":"production ViamindApp, local egui Harness input preparation and actual native WGPU Screenshot event","pid":std::process::id(),"executable":std::env::current_exe().unwrap(),"passes":self.passes,"native_clock_continues_harness_seconds":self.time_offset,"prepared_inputs":self.prepared,"native_controls":["Preview member unfolding","Apply member unfolding","Cancel bus form"],"native_control_bounds":bounds,"full_session_unchanged":true,"complete_document_status_unchanged":true,"document_status":self.status,"native_path_and_bytes_unchanged":true,"source_cursor":self.before.cursor,"preview":self.app.schematic_buses.report,"width":self.size[0],"height":self.size[1],"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"nonopaque_pixels":image.pixels.iter().filter(|p|p.a()!=255).count(),"channels":"Original Color32 RGBA and identical RGB; no unpremultiplication or flattening","preparation_scope":"Read-only shared query/preview prepared through production UI using local egui inputs; fixed viewport, transferred UI memory. Native footer responses matched to exact current painted Rect min/max within0.5px. Compact field typing/Apply independently verified by kittest. No OS input or model mutation.","self_close_requested_after_capture":true,"claims_excluded":["OS input","frame cadence","implicit bus conductance","ERC/DRC certification"]});
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join("native-report.json")).unwrap();
            serde_json::to_writer_pretty(&mut f, &report).unwrap();
            f.sync_all().unwrap();
            self.finished = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.passes == 10 {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        // Continue the Harness clock: transferred Area fade-in state is timed in Harness seconds,
        // and a native clock restarting at zero would keep the bus window fully transparent.
        raw.time = raw.time.map(|t| t + self.time_offset);
        raw.focused = true;
        raw.events.retain(|e| !matches!(e, Event::WindowFocused(false)));
    }
}
fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) -> egui::Rect {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(10);
    h.hover_at(h.get_by_label(label).rect().center());
    h.run_steps(2);
    let r = h.get_by_label(label).rect();
    let clip = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| {
            let egui::Shape::Rect(b) = &s.shape else {
                return None;
            };
            ((b.rect.min - r.min).abs().max_elem() <= 0.5 && (b.rect.max - r.max).abs().max_elem() <= 0.5)
                .then_some(s.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("exact prepared painted control background");
    assert!(clip.contains_rect(r), "{label}: actual ancestor clip {clip:?} vs {r:?}");
    r
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    h.run_steps(3);
    while h.state().schematic_buses.busy() {
        assert!(std::time::Instant::now() < until);
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(6);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    reveal(h, label);
    h.get_by_label(label).focus();
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("existing native required")?);
    let output = PathBuf::from(args.next().ok_or("NEW output directory required")?);
    let size = match args.next() {
        None => [1600, 1000],
        Some(flag) if flag == "--compact" => [640, 400],
        _ => return Err("optional --compact only".into()),
    };
    assert!(args.next().is_none());
    std::fs::create_dir(&output)?;
    let engine = EngineHandle::load(&path)?;
    let path = engine.document_status().path.ok_or("native path")?;
    let native = std::fs::read(&path)?;
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.set_schematic_mode(true);
    let mut h =
        Harness::builder().with_size(egui::vec2(size[0] as f32, size[1] as f32)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(8);
    click(&mut h, "Explicit buses and member unfolding");
    click(&mut h, "Load scalar bus facts");
    wait(&mut h);
    // The bus ComboBox exposes its selected text as the accessibility value, not a label.
    h.get_by_value("New bus").scroll_to_me();
    h.run_steps(8);
    h.get_by_value("New bus").click();
    h.run_steps(4);
    h.get_by_label("DATA[1..0]").click();
    h.run_steps(4);
    let mut prepared = Vec::new();
    for (label, value) in [
        ("Bus expression", "DATA[1..0]"),
        ("Bus start X mm", "0"),
        ("Bus start Y mm", "0"),
        ("Bus end X mm", "20"),
        ("Bus end Y mm", "0"),
        ("Entry bus X mm", "0"),
        ("Entry bus Y mm", "0"),
        ("Entry wire X mm", "2"),
        ("Entry wire Y mm", "2"),
        ("Opposite wire end X mm", "2"),
        ("Opposite wire end Y mm", "8"),
    ] {
        input(&mut h, label, value);
        let r = reveal(&mut h, label);
        prepared.push(
            serde_json::json!({"label":label,"value":value,"response":[r.min.x,r.min.y,r.max.x,r.max.y],"effective_ancestor_clip_verified":true}),
        );
    }
    click(&mut h, "Preview member unfolding");
    wait(&mut h);
    assert!(h.state().schematic_buses.report.is_some(), "{:?}", h.state().schematic_buses.message);
    let mut controls = Vec::new();
    for label in ["Preview member unfolding", "Apply member unfolding", "Cancel bus form"] {
        reveal(&mut h, label);
        h.get_by_label(label).focus();
        h.run_steps(3);
        controls.push(h.ctx.memory(egui::Memory::focused).unwrap());
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let memory = h.ctx.memory(Clone::clone);
    let time_offset = h.ctx.input(|i| i.time);
    let mut app = h.into_state();
    let viewport = [size[0] as f32, size[1] as f32];
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind explicit bus inspection")
            .with_app_id("viamind-bus-inspection")
            .with_inner_size(viewport)
            .with_min_inner_size(viewport)
            .with_max_inner_size(viewport)
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind explicit bus inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspection {
                app,
                engine,
                before,
                status,
                native,
                path,
                output,
                size,
                passes: 0,
                controls,
                prepared: serde_json::json!(prepared),
                time_offset,
                finished: false,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
