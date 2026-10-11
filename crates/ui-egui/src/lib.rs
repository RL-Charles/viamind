//! Viamind desktop UI built on egui and eframe
//!
//! The app shell: a menu bar, a layer side panel, a status bar and the board [`canvas`], rendered
//! with eframe's wgpu backend. The eframe/egui-wgpu setup follows the ArtCraft Craft apps
//! (CADCraft, MIT/Apache-2.0): `default-features = false` plus only the features we use.

pub mod camera;
pub mod canvas;
pub mod mesh_cache;
pub mod scene;

use std::time::Instant;

use egui::{Color32, Ui};

use crate::canvas::Canvas;
use crate::scene::{Layer, Scene};

/// Scene sizes offered in the View menu.
pub const SCENE_SIZES: [usize; 3] = [1_000, 10_000, 100_000];

/// Opens the Viamind window and runs until it is closed.
pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind")
            .with_app_id("viamind")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([640.0, 400.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("Viamind", options, Box::new(|_cc| Ok(Box::new(ViamindApp::default()))))
}

/// Timing of recent frames, for the status bar.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    /// CPU time of the previous full frame (ui, tessellation, paint submit) as reported by eframe.
    pub cpu_ms: Option<f32>,
    /// Time spent building this app's ui last frame.
    pub ui_ms: f32,
    /// Smoothed frames per second while frames are being produced.
    pub fps: f32,
}

/// The Viamind application state.
pub struct ViamindApp {
    pub scene: Scene,
    pub canvas: Canvas,
    pub stats: FrameStats,
    last_frame_time: Option<f64>,
}

impl Default for ViamindApp {
    fn default() -> Self {
        Self::with_scene_size(SCENE_SIZES[1])
    }
}

impl ViamindApp {
    /// An app showing [`Scene::synthetic`] with `n` objects.
    pub fn with_scene_size(n: usize) -> Self {
        Self { scene: Scene::synthetic(n), canvas: Canvas::default(), stats: FrameStats::default(), last_frame_time: None }
    }

    /// Replaces the scene with `n` synthetic objects and zooms to fit it.
    pub fn set_scene_size(&mut self, n: usize) {
        if self.scene.items.len() != n {
            self.scene = Scene::synthetic(n);
            self.canvas.scene_changed();
            self.canvas.fit_pending = true;
        }
    }

    /// Builds one frame of the whole app into `ui` (the root ui of the window).
    ///
    /// `cpu_ms` is the previous frame's CPU time when the host knows it (eframe does).
    pub fn show(&mut self, ui: &mut Ui, cpu_ms: Option<f32>) {
        let start = Instant::now();
        self.update_stats(ui, cpu_ms);

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("layers").default_size(180.0).show(ui, layer_panel);
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas.show(ui, &self.scene));

        self.stats.ui_ms = start.elapsed().as_secs_f32() * 1000.0;
    }

    fn update_stats(&mut self, ui: &Ui, cpu_ms: Option<f32>) {
        self.stats.cpu_ms = cpu_ms;
        let now = ui.input(|i| i.time);
        if let Some(prev) = self.last_frame_time {
            let dt = (now - prev) as f32;
            if dt > 0.0 {
                let fps = 1.0 / dt;
                // Exponential smoothing; restart after idle gaps so the number stays meaningful.
                self.stats.fps = if self.stats.fps == 0.0 || dt > 0.5 { fps } else { self.stats.fps * 0.9 + fps * 0.1 };
            }
        }
        self.last_frame_time = Some(now);
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Quit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Zoom to fit (Home)").clicked() {
                    self.canvas.fit_pending = true;
                    ui.close();
                }
                ui.separator();
                ui.label("Test scene");
                let mut n = self.scene.items.len();
                for size in SCENE_SIZES {
                    ui.radio_value(&mut n, size, format!("{}k objects", size / 1000));
                }
                self.set_scene_size(n);
            });
        });
    }

    fn status_bar(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            match self.canvas.cursor_world {
                Some(p) => ui.monospace(format!("X {:9.3}  Y {:9.3} mm", p.x, p.y)),
                None => ui.monospace(format!("X {:>9}  Y {:>9} mm", "-", "-")),
            };
            ui.separator();
            ui.monospace(format!("zoom {:.3} px/mm", self.canvas.camera.zoom));
            ui.separator();
            ui.monospace(format!("{}/{} drawn", self.canvas.drawn, self.scene.items.len()));
            ui.separator();
            let cpu = self.stats.cpu_ms.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.1}"));
            ui.monospace(format!("frame cpu {cpu} ms  ui {:.1} ms  {:.0} fps", self.stats.ui_ms, self.stats.fps));
        });
    }
}

/// Placeholder layer list (layer visibility and colours arrive with T-0021).
fn layer_panel(ui: &mut Ui) {
    ui.heading("Layers");
    ui.separator();
    for layer in Layer::ALL.into_iter().rev() {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, canvas::layer_color(layer).to_opaque());
            ui.label(layer.name());
        });
    }
    ui.add_space(8.0);
    ui.colored_label(Color32::GRAY, "Pan: middle-drag or space+drag");
    ui.colored_label(Color32::GRAY, "Zoom: scroll wheel");
    ui.colored_label(Color32::GRAY, "Fit: Home");
}

impl eframe::App for ViamindApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let cpu_ms = frame.info().cpu_usage.map(|s| s * 1000.0);
        self.show(ui, cpu_ms);
    }
}
