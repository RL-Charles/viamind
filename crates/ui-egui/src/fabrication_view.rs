//! Local read-only fabrication overlay; all file interpretation uses the shared action.
use crate::{
    ViamindApp,
    actions::UiAction,
    camera::{WorldPos, WorldRect},
    scene::Prim,
};
use egui::{Color32, Stroke, Ui};
use std::sync::mpsc::{self, Receiver};
use viamind_doc::fabrication_view::{FabricationLayer as Layer, FabricationPrimitive as Primitive};
use viamind_engine::{EngineHandle, FabricationInspection, InspectFabricationFiles};

#[derive(Default)]
pub struct FabricationViewUi {
    open: bool,
    paths: [String; 6],
    worker: Option<Receiver<Result<FabricationInspection, String>>>,
    result: Option<FabricationInspection>,
    prepared: Vec<Vec<PreparedPrimitive>>,
    visible: [bool; 6],
    overlay: bool,
    message: Option<String>,
    painted: usize,
}
impl FabricationViewUi {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    pub fn painted_primitives(&self) -> usize {
        self.painted
    }
    fn clear(&mut self) {
        self.result = None;
        self.prepared.clear();
        self.overlay = false;
        self.painted = 0;
    }
    fn sync(&mut self, engine: &EngineHandle) {
        if self.result.as_ref().is_some_and(|r| engine.with_engine(|e| e.project().id != r.expected_project || e.event_cursor() != r.source_cursor)) {
            self.clear();
            self.message = Some("Document changed; inspect files again before showing an overlay.".into());
        }
    }
    fn start(&mut self, engine: EngineHandle, ctx: egui::Context) {
        self.start_with(engine, ctx, |job| std::thread::Builder::new().name("viamind-fabrication-inspection".into()).spawn(job).map(|_| ()));
    }
    fn start_with(&mut self, engine: EngineHandle, ctx: egui::Context, spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>) {
        if self.busy() {
            return;
        }
        if self.paths.iter().any(|s| s.len() > 4096) {
            self.message = Some("File paths must be at most 4096 UTF-8 bytes.".into());
            return;
        }
        let paths = self.paths.iter().filter(|s| !s.trim().is_empty()).map(|s| std::path::PathBuf::from(s.trim())).collect::<Vec<_>>();
        if paths.is_empty() {
            self.message = Some("Enter at least one explicit .gbr or .xnc file path.".into());
            return;
        }
        let (expected_project, expected_cursor) = engine.with_engine(|e| (e.project().id, e.event_cursor()));
        let request = InspectFabricationFiles { paths, expected_project, expected_cursor };
        let (tx, rx) = mpsc::sync_channel(1);
        self.clear();
        let job = Box::new(move || {
            let result = serde_json::to_value(request)
                .map_err(|e| e.to_string())
                .and_then(|input| UiAction::InspectFabricationFiles.run(&engine, input).map_err(|e| e.to_string()))
                .and_then(|value| serde_json::from_value(value).map_err(|e| e.to_string()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        match spawn(job) {
            Ok(()) => {
                self.worker = Some(rx);
                self.message = None;
            }
            Err(e) => self.message = Some(format!("Could not start inspection worker: {e}")),
        }
    }
    fn poll(&mut self, ctx: &egui::Context, engine: &EngineHandle) {
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    match result {
                        Ok(result) => {
                            self.prepared = result.files.iter().map(|f| f.geometry.primitives.iter().map(primitive).collect()).collect();
                            self.visible = [true; 6];
                            self.result = Some(result);
                            self.message = None;
                        }
                        Err(error) => {
                            self.clear();
                            self.message = Some(format!("Inspection failed: {error}"));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.clear();
                    self.message = Some("Inspection worker disconnected; retry inspection.".into());
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            }
        }
        self.sync(engine);
    }
}
struct PreparedPrimitive {
    shape: Prim,
    capsule: Option<Vec<WorldPos>>,
}
fn capsule(a: WorldPos, b: WorldPos, radius: f64) -> Vec<WorldPos> {
    let angle = (b.y - a.y).atan2(b.x - a.x);
    // One convex shape avoids alpha overlap at the two circular end caps.
    [b, a]
        .into_iter()
        .enumerate()
        .flat_map(|(end, center)| {
            (0..=16).map(move |i| {
                let angle = angle - std::f64::consts::FRAC_PI_2 + (end as f64 + f64::from(i) / 16.0) * std::f64::consts::PI;
                WorldPos::new(center.x + radius * angle.cos(), center.y + radius * angle.sin())
            })
        })
        .collect()
}
fn primitive(p: &Primitive) -> PreparedPrimitive {
    let at = |p: viamind_geom::Point| WorldPos::new(p.x.to_mm(), p.y.to_mm());
    let shape = match *p {
        Primitive::Circle { center, diameter } => Prim::Circle { center: at(center), radius: diameter.to_mm() / 2.0 },
        Primitive::Segment { start, end, width } if start == end && width.0 > 0 => Prim::Circle { center: at(start), radius: width.to_mm() / 2.0 },
        Primitive::Segment { start, end, width } => Prim::Segment { a: at(start), b: at(end), width: width.to_mm() },
        Primitive::Rectangle { center, width, height, rotation_mdeg } => {
            let (s, c) = (f64::from(rotation_mdeg) / 1000.0).to_radians().sin_cos();
            let center = at(center);
            Prim::Quad([(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| {
                let x = x * width.to_mm() / 2.0;
                let y = y * height.to_mm() / 2.0;
                WorldPos::new(center.x + x * c + y * s, center.y - x * s + y * c)
            }))
        }
    };
    let capsule = match shape {
        Prim::Segment { a, b, width } if width > 0.0 => Some(capsule(a, b, width / 2.0)),
        _ => None,
    };
    PreparedPrimitive { shape, capsule }
}
fn color(layer: Layer) -> Color32 {
    match layer {
        Layer::TopCopper => Color32::from_rgba_unmultiplied(255, 120, 65, 150),
        Layer::BottomCopper => Color32::from_rgba_unmultiplied(90, 150, 255, 150),
        Layer::TopMaskOpenings => Color32::from_rgba_unmultiplied(235, 80, 230, 115),
        Layer::BottomMaskOpenings => Color32::from_rgba_unmultiplied(80, 235, 170, 115),
        Layer::Profile => Color32::YELLOW,
        Layer::Drill => Color32::from_rgb(120, 255, 255),
    }
}
impl ViamindApp {
    pub fn begin_fabrication_inspection(&mut self) {
        if self.design.is_some() {
            self.fabrication_view.open = true;
        }
    }
    pub(crate) fn fabrication_inspection_window(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else {
            return;
        };
        let engine = design.engine.clone();
        self.fabrication_view.poll(ui.ctx(), &engine);
        if !self.fabrication_view.open {
            return;
        }
        let state = &mut self.fabrication_view;
        let busy = state.busy();
        let mut inspect = false;
        let mut changed = false;
        egui::Window::new("Inspect fabrication files").open(&mut state.open).default_width(550.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Read-only overlay of supported exported Gerber and drill geometry.");
            ui.label("Files are read independently. Matching coordinates do not certify agreement with the board.");
            ui.add_enabled_ui(!busy, |ui| {
                for (i, path) in state.paths.iter_mut().enumerate() {
                    let label = ui.label(format!("Fabrication file {} path", i + 1));
                    changed |= ui
                        .add(egui::TextEdit::singleline(path).desired_width(ui.available_width().min(500.0)).char_limit(4096))
                        .labelled_by(label.id)
                        .changed();
                }
                inspect = ui.button("Inspect selected fabrication files").clicked();
            });
            if busy {
                ui.spinner();
                ui.label("Reading fabrication files…");
            }
            if let Some(message) = &state.message {
                ui.label(message);
            }
            if let Some(result) = &state.result {
                ui.label(format!("Inspected {} files / {} primitives", result.files.len(), state.prepared.iter().map(Vec::len).sum::<usize>()));
                ui.label(format!("Snapshot {} / {}", result.source_cursor.stream_id, result.source_cursor.sequence));
                ui.checkbox(&mut state.overlay, "Show fabrication overlay");
                for (i, file) in result.files.iter().enumerate() {
                    let label = format!("Show fabrication file {} ({:?})", i + 1, file.geometry.layer);
                    ui.colored_label(color(file.geometry.layer), format!("{} — {}", file.path.display(), file.geometry.primitives.len()));
                    ui.checkbox(&mut state.visible[i], label);
                    ui.small(format!("SHA256 {}", file.source_sha256));
                    if file.geometry.negative_file_polarity {
                        ui.label("Negative mask: showing openings only.");
                    }
                    if file.geometry.unverified_common_origin {
                        ui.label("File common origin is user-selected and unverified; no project identity attribute.");
                    }
                }
                ui.label("All sides use top view; file Y is inverted into native downward-Y. No auto offset or mirroring.");
            }
        });
        if changed {
            state.clear();
            state.message = None;
        }
        if inspect {
            state.start(engine, ui.ctx().clone());
        }
    }
    pub(crate) fn paint_fabrication_overlay(&mut self, ui: &Ui, viewport: egui::Rect) {
        let state = &mut self.fabrication_view;
        state.painted = 0;
        let Some(design) = &self.design else {
            return;
        };
        state.sync(&design.engine);
        if !state.open || !state.overlay || self.schematic.active {
            return;
        }
        let Some(result) = &state.result else {
            return;
        };
        let painter = ui.painter().with_clip_rect(viewport);
        let camera = &self.canvas.camera;
        let visible = camera.visible_world_rect(viewport);
        for (i, file) in result.files.iter().enumerate() {
            if !state.visible[i] {
                continue;
            }
            let color = color(file.geometry.layer);
            for prim in &state.prepared[i] {
                if !prim.shape.bounds().intersects(&visible) {
                    continue;
                }
                let screen = |p| camera.world_to_screen(p, viewport);
                if let Some(points) = &prim.capsule {
                    painter.add(egui::Shape::convex_polygon(points.iter().copied().map(screen).collect(), color, Stroke::new(1.0, color)));
                    state.painted += 1;
                    continue;
                }
                match prim.shape {
                    Prim::Circle { center, radius } => {
                        painter.circle_stroke(screen(center), (radius * camera.zoom) as f32, Stroke::new(2.0, color));
                    }
                    Prim::Quad(points) => {
                        painter.add(egui::Shape::convex_polygon(points.map(screen).to_vec(), color, Stroke::new(1.0, color)));
                    }
                    Prim::Segment { a, b, width } => {
                        painter.line_segment([screen(a), screen(b)], Stroke::new(((width * camera.zoom) as f32).max(1.0), color));
                    }
                    Prim::Rect(WorldRect { .. }) | Prim::Via { .. } => unreachable!("inspection prepares only exact file primitives"),
                }
                state.painted += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circular_strokes_include_round_endpoint_envelopes_and_zero_length_disks() {
        use viamind_geom::{Nm, Point};
        let point = |x, y| Point::new(Nm(x), Nm(y));
        let horizontal =
            primitive(&Primitive::Segment { start: point(2_000_000, 3_000_000), end: point(8_000_000, 3_000_000), width: Nm(4_000_000) });
        let points = horizontal.capsule.unwrap();
        assert_eq!(points.len(), 34);
        for expected in [WorldPos::new(0.0, 3.0), WorldPos::new(10.0, 3.0), WorldPos::new(2.0, 1.0), WorldPos::new(8.0, 5.0)] {
            assert!(points.iter().any(|p| (p.x - expected.x).abs() < 1e-12 && (p.y - expected.y).abs() < 1e-12));
        }
        let diagonal =
            primitive(&Primitive::Segment { start: point(-2_000_000, -3_000_000), end: point(2_000_000, 1_000_000), width: Nm(2_000_000) });
        for p in diagonal.capsule.unwrap() {
            // Independent distance-to-segment boundary: every cap vertex is one mm out.
            let t = (((p.x + 2.0) * 4.0 + (p.y + 3.0) * 4.0) / 32.0).clamp(0.0, 1.0);
            let distance = (p.x - (-2.0 + 4.0 * t)).hypot(p.y - (-3.0 + 4.0 * t));
            assert!((distance - 1.0).abs() < 1e-12, "{distance}");
        }
        let zero = primitive(&Primitive::Segment { start: point(-2_000_000, 3_000_000), end: point(-2_000_000, 3_000_000), width: Nm(4_000_000) });
        let Prim::Circle { center, radius } = zero.shape else { panic!("zero-length stroke must show its aperture disk") };
        assert_eq!(center, WorldPos::new(-2.0, 3.0));
        assert!((radius - 2.0).abs() < 1e-12);
        assert!(zero.capsule.is_none());
        let profile = primitive(&Primitive::Segment { start: point(0, 0), end: point(1_000_000, 0), width: Nm(0) });
        assert!(matches!(profile.shape, Prim::Segment { width: 0.0, .. }));
        assert!(profile.capsule.is_none());
    }
    #[test]
    fn exact_odd_rectangles_use_file_ccw_rotation_in_native_downward_y() {
        let prim = primitive(&Primitive::Rectangle {
            center: viamind_geom::Point::new(viamind_geom::Nm(-1234567), viamind_geom::Nm(-2000003)),
            width: viamind_geom::Nm(2000001),
            height: viamind_geom::Nm(1000003),
            rotation_mdeg: 90000,
        });
        let Prim::Quad(points) = prim.shape else { panic!("rectangle required") };
        for (actual, (x, y)) in
            points.iter().zip([(-1.7345685, -1.0000025), (-1.7345685, -3.0000035), (-0.7345655, -3.0000035), (-0.7345655, -1.0000025)])
        {
            assert!((actual.x - x).abs() < 1e-12);
            assert!((actual.y - y).abs() < 1e-12);
        }
    }
    #[test]
    fn retained_worker_handles_duplicate_spawn_disconnect_hidden_and_stale_completion() {
        let engine = EngineHandle::new(viamind_doc::Project::new("keep"));
        let before = engine.session_state();
        let ctx = egui::Context::default();
        let mut ui = FabricationViewUi::default();
        ui.paths[0] = "/new/file.gbr".into();
        ui.start_with(engine.clone(), ctx.clone(), |_| Err(std::io::Error::other("injected spawn")));
        assert!(!ui.busy());
        assert!(ui.message.as_deref().unwrap().contains("injected spawn"));
        assert_eq!(ui.paths[0], "/new/file.gbr");
        let (tx, rx) = mpsc::sync_channel(1);
        ui.worker = Some(rx);
        ui.start_with(engine.clone(), ctx.clone(), |_| panic!("duplicate worker must not spawn"));
        drop(tx);
        ui.poll(&ctx, &engine);
        assert!(!ui.busy());
        assert!(ui.message.as_deref().unwrap().contains("disconnected"));
        let (tx, rx) = mpsc::sync_channel(1);
        ui.worker = Some(rx);
        ui.open = false;
        tx.send(Ok(FabricationInspection { expected_project: before.project.id, source_cursor: before.cursor, files: vec![] })).unwrap();
        ui.poll(&ctx, &engine);
        assert!(ui.result.is_some());
        assert!(!ui.open);
        assert_eq!(engine.session_state(), before);
        UiAction::PlacePart.run(&engine, serde_json::json!({"designator":"PEER","value":"","footprint":"f","x_mm":0,"y_mm":0})).unwrap();
        ui.open = true;
        ui.overlay = true;
        ui.poll(&ctx, &engine);
        assert!(ui.result.is_none());
        assert!(!ui.overlay);
    }
}
