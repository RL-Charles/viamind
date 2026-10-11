//! Actual egui input prepares production UI; native WGPU Screenshot readback closes this host.
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_ui_egui::{ViamindApp, actions::UiAction};
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    ui_passes: usize,
    controls: Vec<egui::Rect>,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        if self.capture.is_none() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.ui_passes += 1;
        let content = ui.ctx().content_rect();
        assert!(self.controls.iter().all(|r| content.contains_rect(*r)), "prepared controls must fit actual native viewport");
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304);
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let path = self.capture.take().expect("one new capture");
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).expect("NEW readback");
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let pixels: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&pixels).unwrap();
            file.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"eframe native WGPU Screenshot event", "pid":std::process::id(), "path":path, "pixels":image.size, "ui_passes":self.ui_passes, "full_session_unchanged":true, "document_status_unchanged":true, "self_close_after_capture":true, "controls":self.controls.iter().map(|r| [r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(), "content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 180, "native screenshot did not complete");
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(6);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(6);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().footprint_definition.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(5);
    assert!(!h.state().footprint_definition.busy());
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args().nth(1).ok_or("native source required")?;
    let output = std::env::args().nth(2).ok_or("NEW output required")?;
    let mode = std::env::args().nth(3).unwrap_or_else(|| "preview".into());
    let width = std::env::args().nth(4).map_or(Ok(1600.0), |v| v.parse::<f32>())?;
    let height = std::env::args().nth(5).map_or(Ok(1100.0), |v| v.parse::<f32>())?;
    let capture = std::env::args_os().nth(6).map(PathBuf::from).ok_or("NEW PPM capture required")?;
    let engine = EngineHandle::new(viamind_doc::Project::new("Detached definition inspection"));
    UiAction::PlacePart
        .run(&engine, serde_json::json!({"designator":"KEEP1","value":"dirty native work preserved","footprint":"Original","x_mm":0,"y_mm":0}))?;
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_definition();
    let mut h = Harness::builder().with_size(egui::vec2(width, height)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    input(&mut h, "Native footprint source path", &source);
    click(&mut h, "Load native footprint definition");
    wait(&mut h);
    input(&mut h, "Footprint definition name", "Original edited odd Nm");
    input(&mut h, "Pad 1 width (mm)", "1.200003");
    input(&mut h, "NEW footprint definition path", &output);
    click(&mut h, "Preview detached footprint");
    assert_eq!(h.state().footprint_definition.preview().unwrap().pads[0].width.0, 1200003);
    if mode == "completed" {
        click(&mut h, "Save NEW footprint definition");
        wait(&mut h);
        assert_eq!(viamind_doc::footprint::load(std::path::Path::new(&output))?.pads[0].width.0, 1200003);
    }
    let label = if mode == "completed" { "Created " } else { "Save NEW footprint definition" };
    h.get_by_label_contains(label).scroll_to_me();
    h.run_steps(8);
    let controls = vec![h.get_by_label_contains(label).rect()];
    let bounds = controls[0];
    assert!(bounds.min.x >= 0.0 && bounds.min.y >= 0.0 && bounds.max.x <= width && bounds.max.y <= height);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind detached footprint inspection")
            .with_app_id("viamind-detached-footprint-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind detached footprint inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture: Some(capture), ui_passes: 0, controls }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
