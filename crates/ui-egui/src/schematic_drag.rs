//! Local connected-move sketches; shared bounded preview and command authorize commits.
use crate::{
    ViamindApp,
    actions::UiAction,
    camera::{Camera, WorldPos},
};
use egui::{Color32, Key, PointerButton, Response, Stroke, Ui};
use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{
    ObjectId,
    schematic::{Coordinate, SchematicWire},
};
use viamind_engine::{DragSchematicSymbol, EngineHandle, EventCursor, SchematicDragPreview, SchematicWireSheetQuery};
use viamind_geom::Nm;

struct LocalDraft {
    request: DragSchematicSymbol,
    origin: Coordinate,
    corners: [Coordinate; 4],
    pins: Vec<Coordinate>,
    incident: Vec<SchematicWire>,
    pointer_start: Option<WorldPos>,
}
impl LocalDraft {
    fn capture(engine: &EngineHandle, sheet_id: ObjectId, symbol_id: ObjectId) -> Result<Self, String> {
        let value = UiAction::SnapshotSchematicWireSheet
            .run(engine, serde_json::to_value(SchematicWireSheetQuery { sheet_id }).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let snapshot: viamind_engine::SchematicWireSnapshot = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let c = snapshot.expected.content.as_ref().ok_or("Select a schematic symbol first")?;
        let symbol = c.symbols.get(&symbol_id).ok_or("Select a schematic symbol first")?;
        if symbol.pins.len() > 256 {
            return Err("Connected move supports at most256 selected symbol pins".into());
        }
        let pins = symbol.pins.iter().map(|pin| symbol.pin_position(pin.position)).collect::<Result<Vec<_>, _>>()?;
        let points = pins.iter().map(|p| (p.x.0, p.y.0)).collect::<BTreeSet<_>>();
        if points.len() != pins.len() {
            return Err("Selected symbol has coincident pin positions".into());
        }
        let mut incident = Vec::new();
        for wire in c.wires.values() {
            if points.contains(&(wire.start.x.0, wire.start.y.0)) || points.contains(&(wire.end.x.0, wire.end.y.0)) {
                if incident.len() == 256 {
                    return Err("Connected move supports at most256 incident wires".into());
                }
                if wire.start == wire.end || (wire.start.x != wire.end.x && wire.start.y != wire.end.y) {
                    return Err("Connected move requires cardinal incident wires".into());
                }
                incident.push(wire.clone());
            }
        }
        Ok(Self {
            request: DragSchematicSymbol {
                expected_project: snapshot.expected_project,
                source_cursor: snapshot.source_cursor,
                sheet_id,
                symbol_id,
                delta: Coordinate { x: Nm(0), y: Nm(0) },
            },
            origin: symbol.position,
            corners: crate::schematic::body_corners(symbol),
            pins,
            incident,
            pointer_start: None,
        })
    }
    fn shifted(&self, p: Coordinate) -> Option<Coordinate> {
        let p = Coordinate { x: Nm(p.x.0.checked_add(self.request.delta.x.0)?), y: Nm(p.y.0.checked_add(self.request.delta.y.0)?) };
        p.validate().ok()?;
        Some(p)
    }
    fn sketch(&self) -> Vec<(Coordinate, Coordinate)> {
        let points = self.pins.iter().map(|p| (p.x.0, p.y.0)).collect::<BTreeSet<_>>();
        let mut lines = Vec::new();
        for wire in &self.incident {
            let a = points.contains(&(wire.start.x.0, wire.start.y.0));
            let b = points.contains(&(wire.end.x.0, wire.end.y.0));
            let start = if a { self.shifted(wire.start) } else { Some(wire.start) };
            let end = if b { self.shifted(wire.end) } else { Some(wire.end) };
            let (Some(start), Some(end)) = (start, end) else {
                continue;
            };
            if a && b || start.x == end.x || start.y == end.y {
                lines.push((start, end));
            } else {
                let (fixed, moving) = if a { (end, start) } else { (start, end) };
                let bend = if wire.start.y == wire.end.y { Coordinate { x: moving.x, y: fixed.y } } else { Coordinate { x: fixed.x, y: moving.y } };
                lines.extend([(fixed, bend), (bend, moving)]);
            }
        }
        lines
    }
}
enum Reply {
    Preview(SchematicDragPreview),
    Applied(EventCursor),
}
struct Pending {
    receiver: Receiver<Result<Reply, String>>,
    request: DragSchematicSymbol,
    applying: bool,
}
pub struct SchematicDragUi {
    draft: Option<LocalDraft>,
    pending: Option<Pending>,
    preview: Option<SchematicDragPreview>,
    pub open: bool,
    pub dx_mm: String,
    pub dy_mm: String,
    pub grid_mm: String,
    pub message: Option<String>,
}
impl Default for SchematicDragUi {
    fn default() -> Self {
        Self { draft: None, pending: None, preview: None, open: false, dx_mm: "0".into(), dy_mm: "0".into(), grid_mm: "1".into(), message: None }
    }
}
impl SchematicDragUi {
    pub fn active(&self) -> bool {
        self.draft.is_some() || self.pending.is_some()
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn report(&self) -> Option<&SchematicDragPreview> {
        self.preview.as_ref()
    }
    pub fn cancel(&mut self) {
        self.draft = None;
        self.preview = None;
        self.open = false;
    }
    pub(crate) fn begin(&mut self, engine: &EngineHandle, sheet: ObjectId, symbol: ObjectId) {
        if self.busy() {
            return;
        }
        match LocalDraft::capture(engine, sheet, symbol) {
            Ok(draft) => {
                self.draft = Some(draft);
                self.preview = None;
                self.dx_mm = "0".into();
                self.dy_mm = "0".into();
                self.message = None;
                self.open = true;
            }
            Err(e) => {
                self.message = Some(e);
                self.open = true;
            }
        }
    }
    fn poll(&mut self, ctx: &egui::Context, engine: Option<&EngineHandle>, target: Option<(ObjectId, ObjectId)>) {
        let current = engine.map(|e| e.with_engine(|e| (e.project().id, e.event_cursor())));
        if self.draft.as_ref().is_some_and(|d| {
            current != Some((d.request.expected_project, d.request.source_cursor)) || target != Some((d.request.sheet_id, d.request.symbol_id))
        }) {
            self.draft = None;
            self.preview = None;
            self.message = Some("Source or selected symbol changed; restart connected move".into());
        }
        if let Some(pending) = &self.pending {
            let request = pending.request;
            let applying = pending.applying;
            match pending.receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok(Reply::Preview(result)) => {
                            if self.draft.as_ref().is_some_and(|d| same_request(d.request, request)) {
                                self.preview = Some(result);
                                self.message = Some("Native graph preserved for this source. Apply explicitly to commit.".into());
                            }
                        }
                        Ok(Reply::Applied(cursor)) => {
                            self.draft = None;
                            self.preview = None;
                            self.message = Some(format!(
                                "Connected move committed at {}:{}{}",
                                cursor.stream_id,
                                cursor.sequence,
                                if current.is_some_and(|(_, c)| c != cursor) { "; newer session changes are present" } else { "" }
                            ));
                        }
                        Err(e) => self.message = Some(e),
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.message = Some(if applying {
                        "Apply worker stopped; inspect current document before retry".into()
                    } else {
                        "Preview worker stopped; retry on the current source".into()
                    });
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(20)),
            }
        }
    }
    fn update_delta(&mut self) -> Result<(), String> {
        let x = crate::schematic::nm(&self.dx_mm)?;
        let y = crate::schematic::nm(&self.dy_mm)?;
        let delta = Coordinate { x: Nm(x), y: Nm(y) };
        delta.validate()?;
        let draft = self.draft.as_mut().ok_or("Start connected move on a selected symbol")?;
        if draft.request.delta != delta {
            draft.request.delta = delta;
            self.preview = None;
        }
        Ok(())
    }
    fn preview(&mut self, engine: &EngineHandle, ctx: &egui::Context) {
        self.preview_with_spawn(engine, ctx, |job| std::thread::Builder::new().name("viamind-schematic-drag-preview".into()).spawn(job));
    }
    fn preview_with_spawn(
        &mut self,
        engine: &EngineHandle,
        ctx: &egui::Context,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<std::thread::JoinHandle<()>>,
    ) {
        if self.busy() {
            return;
        }
        if let Err(e) = self.update_delta() {
            self.message = Some(e);
            return;
        }
        let request = self.draft.as_ref().expect("validated draft").request;
        let work = match engine.prepare_schematic_symbol_drag(request) {
            Ok(work) => work,
            Err(e) => {
                self.message = Some(e.to_string());
                return;
            }
        };
        let (tx, rx) = sync_channel(1);
        let repaint = ctx.clone();
        match spawn(Box::new(move || {
            let result = work.run().map(Reply::Preview).map_err(|e| e.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        })) {
            Ok(_) => {
                self.pending = Some(Pending { receiver: rx, request, applying: false });
                self.preview = None;
                self.message = Some("Checking complete bounded native graph…".into());
            }
            Err(e) => self.message = Some(format!("Could not start connected move preview: {e}")),
        }
    }
    fn apply(&mut self, engine: &EngineHandle, ctx: &egui::Context) {
        if self.busy() || self.preview.is_none() {
            return;
        }
        let Some(draft) = &self.draft else {
            return;
        };
        let request = draft.request;
        let input = match serde_json::to_value(request) {
            Ok(input) => input,
            Err(e) => {
                self.message = Some(e.to_string());
                return;
            }
        };
        let engine = engine.clone();
        let repaint = ctx.clone();
        let (tx, rx) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-schematic-drag-apply".into()).spawn(move || {
            let result = UiAction::DragSchematicSymbol
                .run(&engine, input)
                .map(|_| {
                    Reply::Applied(EventCursor {
                        stream_id: request.source_cursor.stream_id,
                        sequence: request.source_cursor.sequence + u64::from(request.delta.x.0 != 0 || request.delta.y.0 != 0),
                    })
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        }) {
            Ok(_) => {
                self.pending = Some(Pending { receiver: rx, request, applying: true });
                self.message = Some("Applying guarded connected move…".into());
            }
            Err(e) => self.message = Some(format!("Could not start connected move apply: {e}")),
        }
    }
    fn window(&mut self, ctx: &egui::Context, engine: &EngineHandle) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Connected schematic move")
            .id(egui::Id::new("connected_schematic_move"))
            .open(&mut open)
            .vscroll(true)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.label("Drag the selected symbol, or enter a displacement. Release checks the complete native graph; Apply commits explicitly.");
                ui.label("Local cyan geometry is unvalidated until Preview succeeds. Integer Nm; grid nearest ties away from zero.");
                let busy = self.busy();
                let has_draft = self.draft.is_some();
                ui.add_enabled_ui(!busy && has_draft, |ui| {
                    let mut changed = false;
                    for (label, text) in [("Delta X mm", &mut self.dx_mm), ("Delta Y mm", &mut self.dy_mm), ("Grid mm", &mut self.grid_mm)] {
                        let label = ui.label(label);
                        changed |= ui.add(egui::TextEdit::singleline(text).char_limit(32)).labelled_by(label.id).changed();
                    }
                    if changed {
                        self.preview = None;
                        if let Err(e) = self.update_delta() {
                            self.message = Some(e);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!busy && has_draft, egui::Button::new("Preview connected move")).clicked() {
                        self.preview(engine, ctx);
                    }
                    if ui.add_enabled(!busy && self.preview.is_some(), egui::Button::new("Apply connected move")).clicked() {
                        self.apply(engine, ctx);
                    }
                    if ui.add_enabled(!self.pending.as_ref().is_some_and(|p| p.applying), egui::Button::new("Cancel connected move")).clicked() {
                        self.cancel();
                    }
                });
                if let Some(preview) = &self.preview {
                    ui.label(format!(
                        "{} original graph members preserved · {} added wires · {} visits",
                        preview.original_graph_members, preview.added_wire_count, preview.exact_candidate_visits
                    ));
                    ui.label(format!("Source {}:{}", preview.source_cursor.stream_id, preview.source_cursor.sequence));
                }
                if let Some(message) = &self.message {
                    ui.label(message);
                }
                if busy {
                    ui.spinner();
                    ctx.request_repaint_after(std::time::Duration::from_millis(20));
                }
            });
        if !open {
            self.cancel();
        }
    }
    fn canvas(&mut self, ui: &Ui, response: &Response, painter: &egui::Painter, camera: Camera, engine: &EngineHandle) -> bool {
        let active = self.active();
        if !active {
            return false;
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) && !self.pending.as_ref().is_some_and(|p| p.applying) {
            self.cancel();
            return true;
        }
        let mut release = false;
        let busy = self.busy();
        if let Some(draft) = &mut self.draft {
            let viewport = response.rect;
            let screen = |p: Coordinate| camera.world_to_screen(WorldPos::new(p.x.to_mm(), p.y.to_mm()), viewport);
            if !busy
                && response.drag_started_by(PointerButton::Primary)
                && !ui.input(|i| i.key_down(Key::Space))
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let bounds = egui::Rect::from_points(&draft.corners.map(screen)).expand(14.0);
                if bounds.contains(pointer) {
                    draft.pointer_start = Some(camera.screen_to_world(pointer, viewport));
                }
            }
            if let Some(start) = draft.pointer_start
                && response.dragged_by(PointerButton::Primary)
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let current = camera.screen_to_world(pointer, viewport);
                let result = (|| {
                    let grid = crate::schematic::nm(&self.grid_mm)?;
                    if !(1..=10_000_000_000).contains(&grid) {
                        return Err("Grid must be positive and at most10000mm".to_owned());
                    }
                    let snap = |origin: Nm, delta: f64| -> Result<Nm, String> {
                        let n = origin.0 as f64 + delta * 1_000_000.0;
                        if !n.is_finite() || n.abs() > 10_000_000_000.0 {
                            return Err("Target must remain within±10000mm".into());
                        }
                        Ok(Nm(((n / grid as f64).round() as i64) * grid))
                    };
                    let x = snap(draft.origin.x, current.x - start.x)?;
                    let y = snap(draft.origin.y, current.y - start.y)?;
                    let delta = Coordinate { x: Nm(x.0 - draft.origin.x.0), y: Nm(y.0 - draft.origin.y.0) };
                    delta.validate()?;
                    Ok(delta)
                })();
                match result {
                    Ok(delta) => {
                        draft.request.delta = delta;
                        self.dx_mm = format!("{:.6}", delta.x.to_mm());
                        self.dy_mm = format!("{:.6}", delta.y.to_mm());
                        self.preview = None;
                        self.message = None;
                    }
                    Err(e) => self.message = Some(e),
                }
            }
            if response.drag_stopped_by(PointerButton::Primary) && draft.pointer_start.take().is_some() {
                release = true;
            }
            let cyan = Color32::from_rgb(70, 225, 255);
            let corners = draft.corners.iter().filter_map(|p| draft.shifted(*p).map(screen)).collect::<Vec<_>>();
            if corners.len() == 4 {
                painter.add(egui::Shape::closed_line(corners, Stroke::new(2.0, cyan)));
            }
            for (a, b) in draft.sketch() {
                painter.line_segment([screen(a), screen(b)], Stroke::new(2.0, cyan));
            }
        }
        if release {
            self.preview(engine, ui.ctx());
        }
        true
    }
}
fn same_request(a: DragSchematicSymbol, b: DragSchematicSymbol) -> bool {
    a.expected_project == b.expected_project
        && a.source_cursor == b.source_cursor
        && a.sheet_id == b.sheet_id
        && a.symbol_id == b.symbol_id
        && a.delta == b.delta
}
impl ViamindApp {
    pub(crate) fn sync_schematic_drag(&mut self, ctx: &egui::Context) {
        let engine = self.design.as_ref().map(|d| d.engine.clone());
        let target = self.selected_schematic_ids();
        self.schematic_drag.poll(ctx, engine.as_ref(), target);
    }
    pub(crate) fn schematic_drag_toolbar(&mut self, ui: &mut Ui) {
        let target = self.selected_schematic_ids();
        let enabled = !self.files.busy() && !self.schematic_wires.active() && !self.schematic_drag.busy() && target.is_some();
        if ui.add_enabled(enabled, egui::Button::new("Connected move selected symbol")).clicked()
            && let (Some(design), Some((sheet, symbol))) = (&self.design, target)
        {
            self.schematic_drag.begin(&design.engine, sheet, symbol);
            self.clear_schematic_object_draft();
        }
        if self.schematic_drag.active() {
            ui.label("Connected move active. Preview then Apply; Escape cancels the local draft.");
            if ui.button("Show connected move").clicked() {
                self.schematic_drag.open = true;
            }
        }
    }
    pub(crate) fn schematic_drag_window(&mut self, ui: &Ui) {
        if let Some(design) = &self.design {
            self.schematic_drag.window(ui.ctx(), &design.engine);
        }
    }
    pub(crate) fn schematic_drag_canvas(&mut self, ui: &Ui, response: &Response, painter: &egui::Painter, camera: Camera) -> bool {
        self.design.as_ref().is_some_and(|d| self.schematic_drag.canvas(ui, response, painter, camera, &d.engine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (EngineHandle, ObjectId, ObjectId) {
        let e = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/connected-drag.viamind.json")).unwrap());
        let sheet = "00000000-0000-0000-0000-0000000001f6".parse().unwrap();
        let symbol = "00000000-0000-0000-0000-0000000001f7".parse().unwrap();
        (e, sheet, symbol)
    }
    #[test]
    fn retained_hidden_cancel_duplicate_spawn_failure_and_disconnect_preserve_native_source() {
        let (engine, sheet, symbol) = fixture();
        let before = engine.session_state();
        let ctx = egui::Context::default();
        let mut ui = SchematicDragUi::default();
        ui.begin(&engine, sheet, symbol);
        ui.dx_mm = "3".into();
        ui.dy_mm = "5".into();
        ui.preview_with_spawn(&engine, &ctx, |_| Err(std::io::Error::other("controlled failure")));
        assert!(!ui.busy());
        assert!(ui.message.as_ref().unwrap().contains("controlled failure"));
        let (jobs, waiting) = sync_channel(1);
        ui.preview_with_spawn(&engine, &ctx, |job| {
            jobs.send(job).unwrap();
            std::thread::spawn(|| {}).join().unwrap();
            std::thread::Builder::new().spawn(|| {})
        });
        assert!(ui.busy());
        ui.preview_with_spawn(&engine, &ctx, |_| panic!("duplicate preview must not spawn"));
        ui.cancel();
        assert!(ui.busy());
        assert!(!ui.open);
        waiting.recv().unwrap()();
        ui.poll(&ctx, Some(&engine), Some((sheet, symbol)));
        assert!(!ui.busy());
        assert!(ui.report().is_none());
        assert_eq!(engine.session_state(), before);
        ui.begin(&engine, sheet, symbol);
        ui.preview_with_spawn(&engine, &ctx, |job| {
            drop(job);
            std::thread::Builder::new().spawn(|| {})
        });
        ui.poll(&ctx, Some(&engine), Some((sheet, symbol)));
        assert!(!ui.busy());
        assert!(ui.message.as_ref().unwrap().contains("worker stopped"));
        assert_eq!(engine.session_state(), before);
    }
    #[test]
    fn local_fixed_geometry_sketch_matches_expected_asymmetric_three_wire_bends() {
        let (engine, sheet, symbol) = fixture();
        let mut draft = LocalDraft::capture(&engine, sheet, symbol).unwrap();
        draft.request.delta = Coordinate { x: Nm(3_000_000), y: Nm(5_000_000) };
        let lines = draft.sketch();
        assert_eq!(lines.len(), 6);
        let expected =
            [((10, 20), (21, 20)), ((21, 20), (21, 25)), ((30, 21), (25, 21)), ((25, 21), (25, 26)), ((20, 30), (20, 28)), ((20, 28), (23, 28))];
        let key = |p: Coordinate| (p.x.0 / 1_000_000, p.y.0 / 1_000_000);
        let actual = lines.iter().map(|(a, b)| (key(*a), key(*b))).collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}
