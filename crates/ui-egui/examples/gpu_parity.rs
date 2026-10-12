//! Native screenshot parity for persistent GPU geometry and the egui mesh reference.
use egui::{ColorImage, Event, Ui};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Arc, Mutex},
};
use viamind_ui_egui::{
    ViamindApp,
    camera::WorldPos,
    layers::LayerId,
    scene::{Item, Prim},
};
#[path = "support/parity_admission.rs"]
mod parity_admission;
#[path = "support/parity_progress.rs"]
mod parity_progress;
#[path = "support/parity_trace.rs"]
mod parity_trace;
struct Check {
    app: ViamindApp,
    format: eframe::wgpu::TextureFormat,
    step: usize,
    frames: usize,
    waiting: bool,
    reference: Option<Arc<ColorImage>>,
    report: Vec<Value>,
    file: std::fs::File,
    finished: Arc<Mutex<Option<bool>>>,
    trace: Option<parity_trace::Trace>,
    aborted: bool,
    hardened: bool,
}
impl Check {
    fn trace_failure(&mut self, ui: &Ui, error: &str) {
        eprintln!("native parity diagnostic failure: {error}");
        if let Some(trace) = &mut self.trace
            && let Err(error) = trace.finish(Some(error))
        {
            eprintln!("writing diagnostic failure: {error}");
        }
        self.aborted = true;
        *self.finished.lock().unwrap() = Some(false);
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
impl eframe::App for Check {
    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let progress = self.trace.as_ref().and_then(parity_trace::Trace::progress);
        let _callback = progress.as_ref().map(|observed| observed.enter(self.step, self.frames));
        if self.aborted {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.hardened
            && let Some(trace) = &mut self.trace
            && let Err(error) = trace.check_hardened_input(ui.ctx())
        {
            self.trace_failure(ui, &error);
            return;
        }
        if let Some(trace) = &mut self.trace
            && let Err(error) = trace.check_budget(ui.ctx())
        {
            self.trace_failure(ui, &error);
            return;
        }
        if let Some(observed) = &progress {
            observed.phase(parity_progress::Phase::Extraction);
        }
        let image = ui.input(|i| {
            if self.trace.is_some() {
                parity_trace::screenshot(&i.events)
            } else {
                Ok(i.events.iter().find_map(|e| {
                    if let Event::Screenshot { image, user_data, viewport_id } = e {
                        Some((Arc::clone(image), user_data.clone(), *viewport_id))
                    } else {
                        None
                    }
                }))
            }
        });
        let image = match image {
            Ok(image) => image,
            Err(error) => {
                self.trace_failure(ui, &error);
                return;
            }
        };
        if let Some((image, user_data, viewport_id)) = image {
            if let Some(observed) = &progress {
                observed.arrived();
                observed.phase(parity_progress::Phase::Admission);
            }
            if let Some(trace) = &mut self.trace
                && let Err(error) = trace.image(self.step, viewport_id, &user_data, &image, &self.app, ui.ctx())
            {
                self.trace_failure(ui, &error);
                return;
            }
            self.waiting = false;
            if self.step.is_multiple_of(2) {
                self.reference = Some(image);
            } else {
                if let Some(observed) = &progress {
                    observed.phase(parity_progress::Phase::Comparison);
                }
                let reference = self.reference.take().unwrap();
                let rect = self.app.canvas.viewport;
                let ppp = ui.ctx().pixels_per_point();
                let mut sum = 0u64;
                let mut maximum = 0u8;
                let mut changed = 0usize;
                let mut pixels = 0usize;
                for y in ((rect.min.y * ppp).ceil() as usize)..((rect.max.y * ppp).floor() as usize).min(image.size[1]) {
                    for x in ((rect.min.x * ppp).ceil() as usize)..((rect.max.x * ppp).floor() as usize).min(image.size[0]) {
                        let a = reference.pixels[y * reference.size[0] + x].to_array();
                        let b = image.pixels[y * image.size[0] + x].to_array();
                        let differences: [u8; 4] = std::array::from_fn(|i| a[i].abs_diff(b[i]));
                        sum += differences.into_iter().map(u64::from).sum::<u64>();
                        maximum = maximum.max(*differences.iter().max().unwrap());
                        changed += usize::from(differences.iter().any(|&d| d > 2));
                        pixels += 1;
                    }
                }
                self.report.push(json!({"case":self.step/2,"pixels":pixels,"mean_channel_difference":sum as f64/(pixels*4) as f64,"max_channel_difference":maximum,"pixels_over_2_channels":changed,"drawn":self.app.canvas.drawn}));
            }
            self.step += 1;
            self.frames = 0;
            if let Some(observed) = &progress {
                observed.position(self.step, self.frames);
            }
            if self.step == 10 {
                let success = self.report.iter().all(|r| {
                    r["drawn"].as_u64().unwrap() > 0
                        && r["mean_channel_difference"].as_f64().unwrap() < 0.1
                        && r["pixels_over_2_channels"].as_u64().unwrap() < r["pixels"].as_u64().unwrap() / 1000
                });
                serde_json::to_writer_pretty(&mut self.file, &json!({"passed":success,"cases":self.report})).unwrap();
                self.file.write_all(b"\n").unwrap();
                self.file.sync_all().unwrap();
                if let Some(trace) = &mut self.trace
                    && let Err(error) = trace.finish(None)
                {
                    self.trace_failure(ui, &error);
                    return;
                }
                *self.finished.lock().unwrap() = Some(success);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if self.step.is_multiple_of(2) {
                let mut camera = self.app.canvas.camera;
                match self.step / 2 {
                    1 => camera.pan_by_screen(egui::vec2(-19.0, 13.0)),
                    2 => {
                        self.app.layers.set_visible(LayerId::IN1_CU, false);
                        self.app.layers.get_mut(LayerId::F_CU).unwrap().opacity = 0.4;
                        self.app.layers.move_layer(LayerId::B_CU, true);
                    }
                    3 => camera.zoom_about(self.app.canvas.viewport.center(), self.app.canvas.viewport, 1.04),
                    4 => {
                        camera.center = self.app.scene.board.center();
                        camera.zoom_about(self.app.canvas.viewport.center(), self.app.canvas.viewport, 8.0);
                        camera.pan_by_screen(egui::vec2(83.0, -47.0));
                    }
                    _ => {}
                }
                if self.hardened {
                    if let Err(error) = self.app.canvas.set_manual_camera(camera) {
                        self.trace_failure(ui, error);
                        return;
                    }
                } else {
                    self.app.canvas.camera = camera;
                }
            }
        }
        if self.step.is_multiple_of(2) {
            self.app.canvas.cache.disable_gpu();
        } else {
            self.app.canvas.cache.enable_gpu(self.format);
        }
        if let Some(observed) = &progress {
            observed.phase(parity_progress::Phase::Drawing);
        }
        self.app.show(ui, None);
        if let Some(observed) = &progress {
            observed.phase(parity_progress::Phase::TraceDrawing);
        }
        if let Some(trace) = &mut self.trace
            && let Err(error) = trace.draw(self.step, &self.app, ui.ctx())
        {
            self.trace_failure(ui, &error);
            return;
        }
        let ready = if self.hardened {
            match self.trace.as_mut().unwrap().hardened_ready(self.step, ui.ctx()) {
                Ok((ready, completed)) => {
                    self.frames = completed;
                    ready
                }
                Err(error) => {
                    self.trace_failure(ui, &error);
                    return;
                }
            }
        } else {
            self.frames += 1;
            self.frames >= 6
        };
        if let Some(observed) = &progress {
            observed.position(self.step, self.frames);
        }
        if ready && !self.waiting {
            let token = if let Some(trace) = &mut self.trace {
                match trace.request(self.step, &self.app, ui.ctx()) {
                    Ok(token) => token,
                    Err(error) => {
                        self.trace_failure(ui, &error);
                        return;
                    }
                }
            } else {
                Default::default()
            };
            if let Some(observed) = &progress {
                observed.phase(parity_progress::Phase::Submission);
            }
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(token));
            if let Some(observed) = &progress {
                observed.submitted();
            }
            self.waiting = true;
        }
        if let Some(observed) = &progress {
            observed.phase(parity_progress::Phase::Repaint);
        }
        ui.ctx().request_repaint();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let usage = "usage: gpu_parity NEW_REPORT.json [--trace-dir NEW_DIRECTORY [--defer-trace-io [--progress-watchdog [--hardened]]]]";
    let path = args.next().ok_or(usage)?;
    let mut progress_worker = None;
    let mut hardened = false;
    let trace = match args.next() {
        Some(flag) if flag == "--trace-dir" => {
            let directory = args.next().ok_or("--trace-dir requires a NEW directory")?;
            let (deferred, progress) = match args.next() {
                Some(flag) if flag == "--defer-trace-io" => {
                    let progress = match args.next() {
                        Some(flag) if flag == "--progress-watchdog" => {
                            match args.next() {
                                Some(flag) if flag == "--hardened" && args.next().is_none() => hardened = true,
                                None => {}
                                Some(_) => return Err(usage.into()),
                            }
                            true
                        }
                        Some(_) => return Err(usage.into()),
                        None => false,
                    };
                    (true, progress)
                }
                Some(_) => return Err(usage.into()),
                None => (false, false),
            };
            let mut trace = if deferred { parity_trace::Trace::deferred(directory.into())? } else { parity_trace::Trace::new(directory.into())? };
            if hardened {
                trace.enable_hardened()?;
            }
            if progress {
                progress_worker = Some(trace.enable_progress()?);
            }
            Some(trace)
        }
        Some(_) => return Err(usage.into()),
        None => None,
    };
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let finished = Arc::new(Mutex::new(None));
    let shared = Arc::clone(&finished);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind GPU parity")
            .with_app_id("viamind-gpu-parity")
            .with_inner_size([1400.0, 1000.0])
            .with_min_inner_size([1400.0, 1000.0])
            .with_max_inner_size([1400.0, 1000.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let native_result = eframe::run_native(
        "Viamind GPU parity",
        options,
        Box::new(move |cc| {
            Ok(Box::new(Check {
                app: fixture(),
                format: cc.wgpu_render_state.as_ref().unwrap().target_format,
                step: 0,
                frames: 0,
                waiting: false,
                reference: None,
                report: Vec::new(),
                file,
                finished: shared,
                trace,
                aborted: false,
                hardened,
            }))
        }),
    )
    .map_err(|e| e.to_string());
    if let Some(worker) = progress_worker {
        worker.join()?;
    }
    native_result?;
    match *finished.lock().unwrap() {
        Some(true) => Ok(()),
        Some(false) => Err("native canvas parity failed; inspect the report".into()),
        None => Err("parity run closed before completing".into()),
    }
}

fn fixture() -> ViamindApp {
    let mut app = ViamindApp::with_scene_size(5000);
    app.scene.items.push(Item::new(Prim::Via { center: WorldPos::new(150.0, 100.0), radius: 8.0, drill_radius: 3.0 }, LayerId::VIAS));
    app.scene.items.push(Item::new(
        Prim::Quad([WorldPos::new(160.0, 100.0), WorldPos::new(166.0, 104.0), WorldPos::new(163.0, 108.0), WorldPos::new(157.0, 104.0)]),
        LayerId::F_CU,
    ));
    app.scene.ratsnest = vec![[WorldPos::new(150.0, 100.0), WorldPos::new(165.0, 108.0)]];
    app
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardened_five_cases_keep_exact_cameras_and_survive_resize() {
        let ctx = egui::Context::default();
        let mut app = fixture();
        let mut frames = 0usize;
        let mut pump = |app: &mut ViamindApp, width: f32, count: usize| {
            for _ in 0..count {
                frames += 1;
                parity_trace::headless_pass(
                    &ctx,
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1000.0))),
                        time: Some(frames as f64 / 60.0),
                        ..Default::default()
                    },
                    |ui| app.show(ui, None),
                );
            }
        };
        for case in 0..5 {
            let mut camera = app.canvas.camera;
            match case {
                1 => camera.pan_by_screen(egui::vec2(-19.0, 13.0)),
                2 => {
                    app.layers.set_visible(LayerId::IN1_CU, false);
                    app.layers.get_mut(LayerId::F_CU).unwrap().opacity = 0.4;
                    app.layers.move_layer(LayerId::B_CU, true);
                }
                3 => camera.zoom_about(app.canvas.viewport.center(), app.canvas.viewport, 1.04),
                4 => {
                    camera.center = app.scene.board.center();
                    camera.zoom_about(app.canvas.viewport.center(), app.canvas.viewport, 8.0);
                    camera.pan_by_screen(egui::vec2(83.0, -47.0));
                }
                _ => {}
            }
            if case != 0 {
                app.canvas.set_manual_camera(camera).unwrap();
            }
            app.canvas.cache.disable_gpu();
            pump(&mut app, 1400.0, 6);
            let reference = (app.canvas.camera, app.canvas.viewport, app.canvas.drawn);
            app.canvas.cache.enable_gpu(eframe::wgpu::TextureFormat::Bgra8Unorm);
            pump(&mut app, 1400.0, 6);
            assert_eq!((app.canvas.camera, app.canvas.viewport, app.canvas.drawn), reference);
            assert_eq!(reference.2, [5002, 5002, 4665, 4624, 606][case]);
            if case != 0 {
                assert_eq!(reference.0, camera, "manual camera must not be reinterpreted");
                pump(&mut app, 1401.0, 2);
                assert_eq!(app.canvas.camera, camera);
                pump(&mut app, 1400.0, 2);
                assert_eq!((app.canvas.camera, app.canvas.viewport, app.canvas.drawn), reference);
            }
        }
        assert!((app.canvas.camera.zoom - 53.69680915111379).abs() < 1e-10);
        assert!((app.canvas.camera.center.x - 33.8096233769221).abs() < 1e-10);
        assert!((app.canvas.camera.center.y - 25.375284784012628).abs() < 1e-10);
        assert!(frames <= 120);
    }

    #[test]
    fn actual_discarded_passes_do_not_submit_or_inflate_frame_settling() {
        let ctx = egui::Context::default();
        let mut app = fixture();
        let temporary = tempfile::tempdir().unwrap();
        let mut trace = parity_trace::Trace::deferred(temporary.path().join("strict")).unwrap();
        trace.enable_hardened().unwrap();
        let mut requests = Vec::new();
        let mut repeated_passes = 0usize;
        for frame in 0..10 {
            parity_trace::headless_pass(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0))),
                    time: Some(f64::from(frame) / 60.0),
                    ..Default::default()
                },
                |ui| {
                    app.show(ui, None);
                    if ui.ctx().current_pass_index() == 0 {
                        ui.ctx().request_discard("controlled strict harness test");
                    } else {
                        repeated_passes += 1;
                    }
                    trace.draw(0, &app, ui.ctx()).unwrap();
                    let (ready, completed) = trace.hardened_ready(0, ui.ctx()).unwrap();
                    assert!(completed <= frame as usize);
                    if ui.ctx().will_discard() {
                        assert!(!ready);
                    }
                    if ready && requests.is_empty() {
                        let token = trace.request(0, &app, ui.ctx()).unwrap();
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(token));
                        requests.push(ui.ctx().cumulative_frame_nr());
                    }
                },
            );
        }
        assert!(repeated_passes >= 10);
        assert_eq!(requests.len(), 1);
        assert!(requests[0] >= 6);
    }

    #[test]
    fn actual_five_case_layout_is_stable_but_one_pixel_resize_overrides_direct_navigation() {
        let ctx = egui::Context::default();
        let mut app = fixture();
        let mut inputs = 0usize;
        let mut pump = |app: &mut ViamindApp, size: egui::Vec2, steps: usize| {
            for _ in 0..steps {
                inputs += 1;
                parity_trace::headless_pass(
                    &ctx,
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        time: Some(inputs as f64 / 60.0),
                        ..Default::default()
                    },
                    |ui| app.show(ui, None),
                );
            }
        };
        let size = egui::vec2(1400.0, 1000.0);
        let mut states = Vec::new();
        for case in 0..5 {
            match case {
                1 => app.canvas.camera.pan_by_screen(egui::vec2(-19.0, 13.0)),
                2 => {
                    app.layers.set_visible(LayerId::IN1_CU, false);
                    app.layers.get_mut(LayerId::F_CU).unwrap().opacity = 0.4;
                    app.layers.move_layer(LayerId::B_CU, true);
                }
                3 => app.canvas.camera.zoom_about(app.canvas.viewport.center(), app.canvas.viewport, 1.04),
                4 => {
                    app.canvas.camera.center = app.scene.board.center();
                    app.canvas.camera.zoom_about(app.canvas.viewport.center(), app.canvas.viewport, 8.0);
                    app.canvas.camera.pan_by_screen(egui::vec2(83.0, -47.0));
                }
                _ => {}
            }
            app.canvas.cache.disable_gpu();
            pump(&mut app, size, 6);
            let reference = (app.canvas.camera, app.canvas.viewport, app.canvas.drawn);
            app.canvas.cache.enable_gpu(eframe::wgpu::TextureFormat::Bgra8Unorm);
            pump(&mut app, size, 6);
            let gpu = (app.canvas.camera, app.canvas.viewport, app.canvas.drawn);
            eprintln!("headless root UI case {case}: {gpu:?}");
            assert_eq!(reference, gpu, "headless layout state diverged in case {case}");
            assert_eq!(gpu.2, [5002, 5002, 4665, 4624, 606][case]);
            states.push(gpu);
        }
        assert_eq!(states[4].1, egui::Rect::from_min_max(egui::pos2(280.0, 23.0), egui::pos2(1400.0, 977.0)));
        assert!((states[4].0.zoom - 53.69680915111379).abs() < 1e-10);
        assert!((states[4].0.center.x - 33.8096233769221).abs() < 1e-10);
        assert!((states[4].0.center.y - 25.375284784012628).abs() < 1e-10);
        pump(&mut app, egui::vec2(1401.0, 1000.0), 2);
        let resized = app.canvas.camera;
        assert_ne!(resized, states[4].0, "direct navigation leaves startup auto-fit active");
        assert_eq!(resized.center, app.scene.bounds().center());
        assert!(resized.zoom < states[4].0.zoom / 7.0);
        pump(&mut app, size, 2);
        assert_eq!(app.canvas.camera, states[0].0);
        // 64 input steps. This proves a layout hazard, not historical native causality.
    }
}
