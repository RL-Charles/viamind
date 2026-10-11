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
}
impl eframe::App for Check {
    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let image = ui.input(|i| i.events.iter().find_map(|e| if let Event::Screenshot { image, .. } = e { Some(Arc::clone(image)) } else { None }));
        if let Some(image) = image {
            self.waiting = false;
            if self.step.is_multiple_of(2) {
                self.reference = Some(image);
            } else {
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
            if self.step == 10 {
                let success = self.report.iter().all(|r| {
                    r["drawn"].as_u64().unwrap() > 0
                        && r["mean_channel_difference"].as_f64().unwrap() < 0.1
                        && r["pixels_over_2_channels"].as_u64().unwrap() < r["pixels"].as_u64().unwrap() / 1000
                });
                serde_json::to_writer_pretty(&mut self.file, &json!({"passed":success,"cases":self.report})).unwrap();
                self.file.write_all(b"\n").unwrap();
                self.file.sync_all().unwrap();
                *self.finished.lock().unwrap() = Some(success);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if self.step.is_multiple_of(2) {
                match self.step / 2 {
                    1 => self.app.canvas.camera.pan_by_screen(egui::vec2(-19.0, 13.0)),
                    2 => {
                        self.app.layers.set_visible(LayerId::IN1_CU, false);
                        self.app.layers.get_mut(LayerId::F_CU).unwrap().opacity = 0.4;
                        self.app.layers.move_layer(LayerId::B_CU, true);
                    }
                    3 => self.app.canvas.camera.zoom_about(self.app.canvas.viewport.center(), self.app.canvas.viewport, 1.04),
                    4 => {
                        self.app.canvas.camera.center = self.app.scene.board.center();
                        self.app.canvas.camera.zoom_about(self.app.canvas.viewport.center(), self.app.canvas.viewport, 8.0);
                        self.app.canvas.camera.pan_by_screen(egui::vec2(83.0, -47.0));
                    }
                    _ => {}
                }
            }
        }
        if self.step.is_multiple_of(2) {
            self.app.canvas.cache.disable_gpu();
        } else {
            self.app.canvas.cache.enable_gpu(self.format);
        }
        self.app.show(ui, None);
        self.frames += 1;
        if self.frames >= 6 && !self.waiting {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.waiting = true;
        }
        ui.ctx().request_repaint();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("usage: gpu_parity NEW_REPORT.json")?;
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
    eframe::run_native(
        "Viamind GPU parity",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_scene_size(5000);
            app.scene.items.push(Item::new(Prim::Via { center: WorldPos::new(150.0, 100.0), radius: 8.0, drill_radius: 3.0 }, LayerId::VIAS));
            app.scene.items.push(Item::new(
                Prim::Quad([WorldPos::new(160.0, 100.0), WorldPos::new(166.0, 104.0), WorldPos::new(163.0, 108.0), WorldPos::new(157.0, 104.0)]),
                LayerId::F_CU,
            ));
            app.scene.ratsnest = vec![[WorldPos::new(150.0, 100.0), WorldPos::new(165.0, 108.0)]];
            Ok(Box::new(Check {
                app,
                format: cc.wgpu_render_state.as_ref().unwrap().target_format,
                step: 0,
                frames: 0,
                waiting: false,
                reference: None,
                report: Vec::new(),
                file,
                finished: shared,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    match *finished.lock().unwrap() {
        Some(true) => Ok(()),
        Some(false) => Err("native canvas parity failed; inspect the report".into()),
        None => Err("parity run closed before completing".into()),
    }
}
