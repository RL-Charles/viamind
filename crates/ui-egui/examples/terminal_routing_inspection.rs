//! Inspection-only exact-terminal draft prepared with production egui pointer events.
//! Native WGPU readback and immutable session/control assertions; no OS input.
use egui::{Event, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_geom::{Nm, Point};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    ui_passes: usize,
    controls: Vec<egui::Rect>,
    points: Vec<viamind_geom::Point>,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        if self.capture.is_none() {
            // Skip the production dirty-document close dialog once immutable proof is saved.
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.app.routing.terminal_planned_points(), self.points);
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
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
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        // Inspection-only logical focus retention; sends no OS input.
        raw.focused = true;
        raw.events.retain(|e| !matches!(e, egui::Event::WindowFocused(false)));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("native fixture required")?;
    let capture = std::env::args_os().nth(2).map(PathBuf::from).ok_or("NEW P6 capture required")?;
    let width = std::env::args().nth(3).map_or(Ok(1600.0), |v| v.parse::<f32>())?;
    let height = std::env::args().nth(4).map_or(Ok(1000.0), |v| v.parse::<f32>())?;
    let engine = EngineHandle::new(viamind_doc::file::load(path)?);
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_terminal_route();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Cardinal corners").click();
    h.run_steps(3);
    h.get_by_label("Start route").click();
    h.run_steps(5);
    for (x, y) in [(10.000003, 10.000007), (30.000001, 25.000003)] {
        let pos = h.state().canvas.camera.world_to_screen(WorldPos::new(x, y), h.state().canvas.viewport);
        for pressed in [true, false] {
            h.event(Event::PointerMoved(pos));
            h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
            h.run_steps(2);
        }
    }
    let points =
        vec![(10_000_003, 10_000_007), (10_000_000, 10_000_000), (30_000_000, 10_000_000), (30_000_000, 25_000_000), (30_000_001, 25_000_003)]
            .into_iter()
            .map(|(x, y)| Point::new(Nm(x), Nm(y)))
            .collect::<Vec<_>>();
    assert_eq!(h.state().routing.terminal_planned_points(), points);
    h.set_size(vec2(width, height));
    h.run_steps(8);
    let controls = ["Finish terminal route", "Cancel terminal route"].into_iter().map(|label| h.get_by_label(label).rect()).collect::<Vec<_>>();
    assert!(controls.iter().all(|r| r.min.x >= 0.0 && r.min.y >= 0.0 && r.max.x <= width && r.max.y <= height));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    println!(
        "{}",
        serde_json::json!({"actual_egui_pointer_preparation":true,"local_preview_unchecked":true,"points_nm":points,"logical_focus_retained_only_in_inspection":true,"os_input":false})
    );
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind existing copper terminal inspection")
            .with_app_id("viamind-terminal-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind existing copper terminal inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture: Some(capture), ui_passes: 0, controls, points }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
