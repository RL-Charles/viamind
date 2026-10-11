//! Viamind desktop UI built on egui and eframe
//!
//! The app shell: a menu bar, a layer side panel, a status bar and the board [`canvas`], rendered
//! with eframe's wgpu backend. The eframe/egui-wgpu setup follows the ArtCraft Craft apps
//! (CADCraft, MIT/Apache-2.0): `default-features = false` plus only the features we use.

pub mod actions;
pub mod annotation;
pub mod board_scene;
pub mod camera;
pub mod canvas;
pub mod copper;
pub mod drc;
pub mod erc;
pub mod fabrication;
pub mod files;
mod gpu_tiles;
pub mod gpu_timing;
pub mod layer_panel;
pub mod layers;
pub mod mesh_cache;
pub mod outlines;
mod pad_routing;
pub mod pads;
pub mod parts;
pub mod pcb_sync;
pub mod placement;
pub mod routing;
pub mod scene;
pub mod schematic;
pub mod selection;

use std::sync::mpsc::Receiver;
use std::time::Instant;

use crate::actions::UiAction;
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
    eframe::run_native(
        "Viamind",
        options,
        Box::new(|cc| {
            let mut app = ViamindApp::default();
            app.configure_renderer(cc);
            Ok(Box::new(app))
        }),
    )
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
            let mut app = ViamindApp::with_design(engine, mcp_url);
            app.configure_renderer(cc);
            Ok(Box::new(app))
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
    pub schematic: schematic::SchematicUi,
    pub annotation: annotation::AnnotationUi,
    pub pcb_sync: pcb_sync::PcbSyncUi,
    pub scene: Scene,
    /// How the canvas draws the scene's layers; edited in the layer panel.
    pub layers: LayerStack,
    pub canvas: Canvas,
    pub stats: FrameStats,
    pub selection: selection::Selection,
    /// The live design, when the app was opened on one (otherwise the synthetic test scene is shown).
    pub design: Option<Design>,
    pub fabrication: fabrication::FabricationUi,
    pub files: files::FileUi,
    pub parts: parts::PartUi,
    pub outlines: outlines::OutlineUi,
    pub copper: copper::CopperUi,
    pub routing: routing::RoutingUi,
    pub pads: pads::PadsUi,
    pub drc: drc::DrcUi,
    pub erc: erc::ErcUi,
    last_frame_time: Option<f64>,
}

impl Default for ViamindApp {
    fn default() -> Self {
        Self::with_scene_size(SCENE_SIZES[1])
    }
}

impl ViamindApp {
    /// Use native persistent GPU layers when the wgpu renderer is available.
    pub fn configure_renderer(&mut self, cc: &eframe::CreationContext<'_>) {
        if let Some(render) = &cc.wgpu_render_state {
            self.canvas.cache.enable_gpu(render.target_format);
        } else {
            self.canvas.cache.disable_gpu();
        }
    }

    /// An app showing [`Scene::synthetic`] with `n` objects.
    pub fn with_scene_size(n: usize) -> Self {
        Self {
            schematic: schematic::SchematicUi::default(),
            annotation: annotation::AnnotationUi::default(),
            pcb_sync: pcb_sync::PcbSyncUi::default(),
            scene: Scene::synthetic(n),
            layers: LayerStack::pcb_default(4),
            canvas: Canvas::default(),
            stats: FrameStats::default(),
            selection: selection::Selection::default(),
            design: None,
            fabrication: fabrication::FabricationUi::default(),
            files: files::FileUi::default(),
            parts: parts::PartUi::default(),
            outlines: outlines::OutlineUi::default(),
            copper: copper::CopperUi::default(),
            routing: routing::RoutingUi::default(),
            pads: pads::PadsUi::default(),
            drc: drc::DrcUi::default(),
            erc: erc::ErcUi::default(),
            last_frame_time: None,
        }
    }

    /// An app showing the live design in `engine`.
    pub fn with_design(engine: EngineHandle, mcp_url: Option<String>) -> Self {
        let changes = engine.subscribe();
        let scene = engine.read(board_scene::scene_from_project);
        let schematic = schematic::SchematicUi::with_active(
            engine.read(|p| !p.schematic.sheets.is_empty() && p.board.parts.is_empty() && p.board.tracks.is_empty() && p.board.vias.is_empty()),
        );
        let mut canvas = Canvas::default();
        canvas.fit_pending = true;
        Self {
            schematic,
            annotation: annotation::AnnotationUi::default(),
            pcb_sync: pcb_sync::PcbSyncUi::default(),
            scene,
            layers: LayerStack::pcb_default(4),
            canvas,
            stats: FrameStats::default(),
            selection: selection::Selection::default(),
            design: Some(Design { engine, changes, mcp_url, last_change: None, change_count: 0 }),
            fabrication: fabrication::FabricationUi::default(),
            files: files::FileUi::default(),
            parts: parts::PartUi::default(),
            outlines: outlines::OutlineUi::default(),
            copper: copper::CopperUi::default(),
            routing: routing::RoutingUi::default(),
            pads: pads::PadsUi::default(),
            drc: drc::DrcUi::default(),
            erc: erc::ErcUi::default(),
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
                ChangeKind::Replaced => {
                    self.selection.cancel_gestures();
                    self.schematic.active = design
                        .engine
                        .read(|p| !p.schematic.sheets.is_empty() && p.board.parts.is_empty() && p.board.tracks.is_empty() && p.board.vias.is_empty());
                    self.canvas.fit_pending = true;
                    "opened document"
                }
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
            self.selection.invalidate();
        }
    }

    /// Undo the latest change from anyone (the human can undo agent edits).
    pub fn undo_design(&mut self) {
        self.routing.cancel();
        if let Some(design) = &self.design {
            let _ = UiAction::Undo.run(&design.engine, json!({}));
        }
    }

    /// Redo the latest undone change.
    pub fn redo_design(&mut self) {
        self.routing.cancel();
        if let Some(design) = &self.design {
            let _ = UiAction::Redo.run(&design.engine, json!({}));
        }
    }

    /// Places a 10k 0603 resistor with the next free designator near the board centre (Edit menu).
    pub fn place_test_resistor(&mut self) {
        if self.routing.editing() {
            return;
        }
        let Some(design) = &self.design else { return };
        let n = design.engine.read(|p| (1..).find(|n| p.board.part_by_designator(&format!("R{n}")).is_none()).unwrap_or(1));
        let x = 20.0 + f64::from((n - 1) % 10) * 6.0;
        let y = 20.0 + f64::from((n - 1) / 10) * 6.0;
        let input = json!({ "designator": format!("R{n}"), "value": "10k", "footprint": "R_0603", "x_mm": x, "y_mm": y });
        if let Err(e) = UiAction::PlacePart.run(&design.engine, input)
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
        self.file_shortcuts(ui);
        if self.design.is_some()
            && !self.files.busy()
            && !self.parts.placing()
            && !self.outlines.editing()
            && !self.pads.editing()
            && !self.copper.editing()
            && !self.routing.editing()
            && !ui.ctx().egui_wants_keyboard_input()
        {
            if ui.input_mut(|i| i.consume_shortcut(&REDO)) {
                self.redo_design();
            } else if ui.input_mut(|i| i.consume_shortcut(&UNDO)) {
                self.undo_design();
            }
        }
        self.sync_design();

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left(if self.schematic.active { "schematic_tools" } else { "layers" })
            .default_size(if self.schematic.active { 380.0 } else { 280.0 })
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.schematic.active {
                        self.schematic_panel(ui);
                        return;
                    }
                    layer_panel::show(ui, &mut self.layers);
                    if let Some(design) = &self.design {
                        self.routing.panel(ui, &design.engine);
                    }
                    if self.design.is_some() {
                        ui.separator();
                        self.part_properties(ui);
                    }
                });
            });
        self.pcb_sync_window(ui);
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            if self.schematic.active {
                self.schematic_canvas(ui);
                return;
            }
            let response = self.canvas.show(ui, &self.scene, &self.layers);
            if let Some(design) = &self.design {
                self.selection.sync(&design.engine, &self.layers);
                let placement_enabled = !self.schematic.active
                    && !self.files.busy()
                    && !self.parts.placing()
                    && !self.outlines.editing()
                    && !self.pads.editing()
                    && !self.copper.editing()
                    && !self.routing.editing();
                self.selection.show_enabled(ui, &response, &self.canvas.camera, &design.engine, placement_enabled);
                let routing_enabled =
                    !self.files.busy() && !self.parts.placing() && !self.outlines.editing() && !self.pads.editing() && !self.copper.editing();
                self.routing.canvas(ui, &response, &self.canvas.camera, &design.engine, &self.layers, routing_enabled);
            }
            self.paint_labels(ui);
        });
        self.file_dialogs(ui);
        self.placement_dialog(ui);
        self.pads_dialog(ui);
        self.outline_dialog(ui);
        self.copper_dialog(ui);
        self.route_dialog(ui);
        self.drc_window(ui);
        self.erc_window(ui);
        self.fabrication_window(ui);
        if let Some(design) = &self.design {
            let status = design.engine.document_status();
            let name =
                status.path.as_ref().and_then(|path| path.file_name()).map_or_else(|| "Untitled".into(), |name| name.to_string_lossy().into_owned());
            ui.send_viewport_cmd(egui::ViewportCommand::Title(format!("{}{} — Viamind", if status.dirty { "*" } else { "" }, name)));
        }

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
            if self.design.is_some() {
                if ui.selectable_label(!self.schematic.active, "PCB").clicked() {
                    self.set_schematic_mode(false);
                }
                if ui.selectable_label(self.schematic.active, "Schematic").clicked() {
                    self.set_schematic_mode(true);
                }
            }
            ui.menu_button("File", |ui| {
                if self.design.is_some() {
                    if ui.add(egui::Button::new("New…").shortcut_text(ui.ctx().format_shortcut(&files::NEW))).clicked() {
                        self.begin_document(true);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Open…").shortcut_text(ui.ctx().format_shortcut(&files::OPEN))).clicked() {
                        self.begin_document(false);
                        ui.close();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Save").shortcut_text(ui.ctx().format_shortcut(&files::SAVE))).clicked() {
                        self.save_design();
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Save As…").shortcut_text(ui.ctx().format_shortcut(&files::SAVE_AS))).clicked() {
                        self.begin_save_as();
                        ui.close();
                    }
                    ui.separator();
                }
                if ui.button("Quit").clicked() {
                    self.request_close(ui.ctx());
                    ui.close();
                }
            });
            if self.design.is_some() {
                ui.menu_button("Edit", |ui| {
                    let (can_undo, can_redo) = self.design.as_ref().map_or((false, false), |d| (d.engine.can_undo(), d.engine.can_redo()));
                    let undo = egui::Button::new("Undo").shortcut_text(ui.ctx().format_shortcut(&UNDO));
                    if ui.add_enabled(can_undo, undo).clicked() {
                        self.undo_design();
                        ui.close();
                    }
                    let redo = egui::Button::new("Redo").shortcut_text(ui.ctx().format_shortcut(&REDO));
                    if ui.add_enabled(can_redo, redo).clicked() {
                        self.redo_design();
                        ui.close();
                    }
                    if self.schematic.active {
                        return;
                    }
                    if let Some(design) = &self.design {
                        self.selection.placement_menu(
                            ui,
                            &design.engine,
                            !self.schematic.active
                                && !self.files.busy()
                                && !self.parts.placing()
                                && !self.outlines.editing()
                                && !self.pads.editing()
                                && !self.copper.editing()
                                && !self.routing.editing(),
                        );
                    }
                    ui.separator();
                    ui.label("Selection tool");
                    ui.radio_value(&mut self.selection.tool, selection::SelectionTool::Box, "Box");
                    ui.radio_value(&mut self.selection.tool, selection::SelectionTool::Lasso, "Lasso");
                    ui.label("Shift adds · Ctrl toggles · Ctrl+Shift removes");
                    if ui.button("Select all visible (Ctrl+A)").clicked() {
                        if let Some(design) = &self.design {
                            self.selection.select_all(&design.engine);
                        }
                        ui.close();
                    }
                    if ui.button("Clear selection (Esc)").clicked() {
                        if let Some(design) = &self.design {
                            self.selection.clear(&design.engine);
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Place part…").clicked() {
                        self.begin_place_part();
                        ui.close();
                    }
                    if ui.button("Place test resistor").clicked() {
                        self.place_test_resistor();
                        ui.close();
                    }
                });
            }
            if self.design.is_some() {
                ui.menu_button("Inspect", |ui| {
                    if ui.button("Electrical rule check…").clicked() {
                        self.begin_erc();
                        ui.close();
                    }
                });
            }
            if self.design.is_some() && ui.button("Synchronize…").clicked() {
                self.begin_pcb_sync();
            }
            if self.design.is_some() && !self.schematic.active {
                ui.menu_button("Board", |ui| {
                    if ui.button("Export fabrication…").clicked() {
                        self.begin_fabrication_export();
                        ui.close();
                    }
                    if ui.button("Design rule check…").clicked() {
                        self.begin_drc();
                        ui.close();
                    }
                    if ui.button("Route pad connection…").clicked() {
                        self.begin_pad_route();
                        ui.close();
                    }
                    if ui.button("Route polyline…").clicked() {
                        self.begin_route();
                        ui.close();
                    }
                    if ui.button("Copper tracks and vias…").clicked() {
                        self.begin_copper();
                        ui.close();
                    }
                    if ui.button("Board outline…").clicked() {
                        self.begin_board_outline();
                        ui.close();
                    }
                });
            }
            ui.menu_button("View", |ui| {
                if ui.button("Zoom to fit (Home)").clicked() {
                    if self.schematic.active {
                        self.fit_schematic();
                    } else {
                        self.canvas.fit_pending = true;
                    }
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
        if self.schematic.active {
            self.schematic_status(ui);
            return;
        }
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
                let status = design.engine.document_status();
                ui.separator();
                ui.label(format!("{} connection guides", self.scene.ratsnest.len())).on_hover_text(
                    "Cyan terminal connection guides estimate routing work. Touching copper is not analyzed; guides are not copper or a DRC result.",
                );
                ui.separator();
                ui.label(if status.dirty {
                    "Unsaved changes"
                } else if status.path.is_none() {
                    "Not saved"
                } else {
                    "Saved"
                });
                if let Some(path) = &status.path {
                    ui.label(path.display().to_string());
                }
                if let Some(message) = &self.files.message {
                    ui.label(message);
                }
                ui.separator();
                let parts = design.engine.read(|p| p.board.parts.len());
                ui.monospace(format!("{parts} parts · {} selected", design.engine.selection().ids.len()));
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
