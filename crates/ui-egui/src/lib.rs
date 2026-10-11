//! Viamind desktop UI built on egui and eframe
//!
//! The app shell: a menu bar, a layer side panel, a status bar and the board [`canvas`], rendered
//! with eframe's wgpu backend. The eframe/egui-wgpu setup follows the ArtCraft Craft apps
//! (CADCraft, MIT/Apache-2.0): `default-features = false` plus only the features we use.

pub mod board_scene;
pub mod camera;
pub mod canvas;
pub mod layer_panel;
pub mod layers;
pub mod mesh_cache;
pub mod scene;

use std::sync::mpsc::Receiver;
use std::time::Instant;

use egui::{Key, KeyboardShortcut, Modifiers, Ui};
use serde_json::json;
use viamind_engine::{ChangeEvent, ChangeKind, EngineHandle, Origin};

use crate::canvas::Canvas;
use crate::layers::LayerStack;
use crate::scene::Scene;

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

/// Opens the Viamind window on a live design. Agents connected to `mcp_url` edit the same
/// [`EngineHandle`], and their edits repaint the window as they land.
pub fn run_design(engine: EngineHandle, mcp_url: Option<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind")
            .with_app_id("viamind")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([640.0, 400.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            engine.on_change(move || ctx.request_repaint());
            Ok(Box::new(ViamindApp::with_design(engine, mcp_url)))
        }),
    )
}

/// Shortcut for undo.
pub const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
/// Shortcut for redo.
pub const REDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);

/// The live design shown by the app, shared with connected agents.
pub struct Design {
    /// The engine; every edit, from the UI or an agent, goes through it.
    pub engine: EngineHandle,
    changes: Receiver<ChangeEvent>,
    /// Where agents connect, shown in the status bar.
    pub mcp_url: Option<String>,
    /// One-line description of the latest change, e.g. `applied add_part by agent claude-code#1`.
    pub last_change: Option<String>,
    /// Number of change events received, for tests and the status bar.
    pub change_count: u64,
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
    /// How the canvas draws the scene's layers; edited in the layer panel.
    pub layers: LayerStack,
    pub canvas: Canvas,
    pub stats: FrameStats,
    /// The live design, when the app was opened on one (otherwise the synthetic test scene is shown).
    pub design: Option<Design>,
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
        Self {
            scene: Scene::synthetic(n),
            layers: LayerStack::pcb_default(4),
            canvas: Canvas::default(),
            stats: FrameStats::default(),
            design: None,
            last_frame_time: None,
        }
    }

    /// An app showing the live design in `engine`.
    pub fn with_design(engine: EngineHandle, mcp_url: Option<String>) -> Self {
        let changes = engine.subscribe();
        let scene = engine.read(board_scene::scene_from_project);
        let mut canvas = Canvas::default();
        canvas.fit_pending = true;
        Self {
            scene,
            layers: LayerStack::pcb_default(4),
            canvas,
            stats: FrameStats::default(),
            design: Some(Design { engine, changes, mcp_url, last_change: None, change_count: 0 }),
            last_frame_time: None,
        }
    }

    /// Applies pending change events from the engine: rebuilds the scene once if anything changed.
    pub fn sync_design(&mut self) {
        let Some(design) = &mut self.design else { return };
        let mut changed = false;
        while let Ok(event) = design.changes.try_recv() {
            changed = true;
            design.change_count += 1;
            let kind = match event.kind {
                ChangeKind::Applied => "applied",
                ChangeKind::Undone => "undid",
                ChangeKind::Redone => "redid",
            };
            let who = match &event.origin {
                Origin::User => "you".to_owned(),
                Origin::Agent { id } => format!("agent {id}"),
            };
            design.last_change = Some(format!("{kind} tx {} ({} object(s)) by {who}", event.tx, event.deltas.len()));
        }
        if changed {
            self.scene = design.engine.read(board_scene::scene_from_project);
            self.canvas.scene_changed();
        }
    }

    /// Undo the latest change from anyone (the human can undo agent edits).
    pub fn undo(&mut self) {
        if let Some(design) = &self.design {
            let _ = design.engine.undo(Origin::User);
        }
    }

    /// Redo the latest undone change.
    pub fn redo(&mut self) {
        if let Some(design) = &self.design {
            let _ = design.engine.redo(Origin::User);
        }
    }

    /// Places a 10k 0603 resistor with the next free designator near the board centre (Edit menu).
    pub fn place_test_resistor(&mut self) {
        let Some(design) = &self.design else { return };
        let n = design.engine.read(|p| (1..).find(|n| p.board.part_by_designator(&format!("R{n}")).is_none()).unwrap_or(1));
        let x = 20.0 + f64::from((n - 1) % 10) * 6.0;
        let y = 20.0 + f64::from((n - 1) / 10) * 6.0;
        let input = json!({ "designator": format!("R{n}"), "value": "10k", "footprint": "R_0603", "x_mm": x, "y_mm": y });
        if let Err(e) = design.engine.execute("add_part", input, Origin::User)
            && let Some(design) = &mut self.design
        {
            design.last_change = Some(format!("place failed: {e}"));
        }
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
        if self.design.is_some() {
            if ui.input_mut(|i| i.consume_shortcut(&REDO)) {
                self.redo();
            } else if ui.input_mut(|i| i.consume_shortcut(&UNDO)) {
                self.undo();
            }
        }
        self.sync_design();

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("layers").default_size(250.0).show(ui, |ui| layer_panel::show(ui, &mut self.layers));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            self.canvas.show(ui, &self.scene, &self.layers);
            self.paint_labels(ui);
        });

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

    /// Draws part designators over the canvas once parts are large enough to read.
    fn paint_labels(&self, ui: &Ui) {
        let Some(design) = &self.design else { return };
        let zoom = self.canvas.camera.zoom;
        if zoom < 4.0 {
            return;
        }
        let viewport = self.canvas.viewport;
        let painter = ui.painter_at(viewport);
        let size = (zoom * 0.8).clamp(9.0, 28.0) as f32;
        let font = egui::FontId::proportional(size);
        let color = egui::Color32::from_rgb(240, 240, 240);
        design.engine.read(|project| {
            for part in project.board.parts.values() {
                let p = camera::WorldPos::new(part.position.x.to_mm(), part.position.y.to_mm());
                let at = self.canvas.camera.world_to_screen(p, viewport);
                if viewport.contains(at) {
                    painter.text(at, egui::Align2::CENTER_CENTER, &part.designator, font.clone(), color);
                }
            }
        });
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Quit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if self.design.is_some() {
                ui.menu_button("Edit", |ui| {
                    let (can_undo, can_redo) = self.design.as_ref().map_or((false, false), |d| (d.engine.can_undo(), d.engine.can_redo()));
                    let undo = egui::Button::new("Undo").shortcut_text(ui.ctx().format_shortcut(&UNDO));
                    if ui.add_enabled(can_undo, undo).clicked() {
                        self.undo();
                        ui.close();
                    }
                    let redo = egui::Button::new("Redo").shortcut_text(ui.ctx().format_shortcut(&REDO));
                    if ui.add_enabled(can_redo, redo).clicked() {
                        self.redo();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Place test resistor").clicked() {
                        self.place_test_resistor();
                        ui.close();
                    }
                });
            }
            ui.menu_button("View", |ui| {
                if ui.button("Zoom to fit (Home)").clicked() {
                    self.canvas.fit_pending = true;
                    ui.close();
                }
                if self.design.is_none() {
                    ui.separator();
                    ui.label("Test scene");
                    let mut n = self.scene.items.len();
                    for size in SCENE_SIZES {
                        ui.radio_value(&mut n, size, format!("{}k objects", size / 1000));
                    }
                    self.set_scene_size(n);
                }
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
            let active = self.layers.get(self.layers.active()).map_or("-", |l| l.name.as_str());
            ui.monospace(format!("layer {active}"));
            ui.separator();
            let cpu = self.stats.cpu_ms.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.1}"));
            ui.monospace(format!("frame cpu {cpu} ms  ui {:.1} ms  {:.0} fps", self.stats.ui_ms, self.stats.fps));
            if let Some(design) = &self.design {
                ui.separator();
                let parts = design.engine.read(|p| p.board.parts.len());
                ui.monospace(format!("{parts} parts"));
                ui.separator();
                ui.monospace(design.mcp_url.as_deref().map_or_else(|| "MCP off".to_owned(), |url| format!("MCP {url}")));
                if let Some(last) = &design.last_change {
                    ui.separator();
                    ui.monospace(last);
                }
            }
        });
    }
}

impl eframe::App for ViamindApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let cpu_ms = frame.info().cpu_usage.map(|s| s * 1000.0);
        self.show(ui, cpu_ms);
    }
}
