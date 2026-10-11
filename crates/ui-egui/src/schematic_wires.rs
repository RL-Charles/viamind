//! Local cardinal wire previews; all persisted segments use the shared command.
use crate::{
    ViamindApp,
    actions::UiAction,
    camera::{Camera, WorldPos},
};
use egui::{Color32, Key, PointerButton, Response, Stroke, Ui};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, schematic::Coordinate};
use viamind_engine::{DrawSchematicWirePolyline, EngineHandle, EventCursor, SchematicWireSheetQuery, SchematicWireSnapshot};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};

struct Anchor {
    id: ObjectId,
    kind: u8,
    at: Coordinate,
}
struct WireDraft {
    snapshot: SchematicWireSnapshot,
    anchors: Vec<Anchor>,
    index: SpatialIndex<usize>,
    points: Vec<Coordinate>,
    clicks: Vec<usize>,
}
impl WireDraft {
    fn new(snapshot: SchematicWireSnapshot) -> Result<Self, String> {
        let mut anchors = Vec::new();
        if let Some(c) = &snapshot.expected.content {
            for s in c.symbols.values() {
                for pin in &s.pins {
                    anchors.push(Anchor { id: pin.id, kind: 0, at: s.pin_position(pin.position)? });
                }
            }
            for wire in c.wires.values() {
                for at in [wire.start, wire.end] {
                    anchors.push(Anchor { id: wire.id, kind: 1, at });
                }
            }
            for j in c.junctions.values() {
                anchors.push(Anchor { id: j.id, kind: 2, at: j.position });
            }
            for l in c.labels.values() {
                anchors.push(Anchor { id: l.id, kind: 3, at: l.position });
            }
        }
        if anchors.len() > 30_000 {
            return Err("Wire tool supports at most 30000 attachment anchors".into());
        }
        let index = SpatialIndex::bulk_load(anchors.iter().enumerate().map(|(i, a)| {
            let p = Point::new(a.at.x, a.at.y);
            (i, Rect::new(p, p))
        }));
        Ok(Self { snapshot, anchors, index, points: vec![], clicks: vec![] })
    }
    fn target(&self, camera: Camera, viewport: egui::Rect, pointer: egui::Pos2, step: i64) -> Result<(Coordinate, bool), String> {
        let raw = camera.screen_to_world(pointer, viewport);
        if !raw.x.is_finite() || !raw.y.is_finite() || raw.x.abs() > 10000.0 || raw.y.abs() > 10000.0 {
            return Err("Wire coordinates must stay within ±10000 mm".into());
        }
        let raw = Coordinate { x: Nm((raw.x * 1_000_000.0).round() as i64), y: Nm((raw.y * 1_000_000.0).round() as i64) };
        let tolerance = Nm((12.0 / camera.zoom * 1_000_000.0).ceil().clamp(0.0, 20_000_000_000.0) as i64);
        let near = self
            .index
            .query_point(Point::new(raw.x, raw.y), tolerance)
            .map(|(i, _)| &self.anchors[i])
            .map(|a| (a, camera.world_to_screen(world(a.at), viewport).distance_sq(pointer)))
            .filter(|(_, d)| *d <= 144.0)
            .min_by(|(a, da), (b, db)| da.total_cmp(db).then_with(|| (a.kind, a.id, a.at.x.0, a.at.y.0).cmp(&(b.kind, b.id, b.at.x.0, b.at.y.0))));
        if let Some((a, _)) = near {
            return Ok((a.at, true));
        }
        let snap = |n: i64| {
            let n = i128::from(n);
            let step = i128::from(step);
            (((n.abs() + step / 2) / step) * step * n.signum()) as i64
        };
        let p = Coordinate { x: Nm(snap(raw.x.0)), y: Nm(snap(raw.y.0)) };
        p.validate()?;
        Ok((p, false))
    }
    fn additions(&self, target: Coordinate, horizontal: bool) -> Vec<Coordinate> {
        let Some(last) = self.points.last().copied() else { return vec![target] };
        if last == target {
            return vec![];
        }
        if last.x == target.x || last.y == target.y {
            return vec![target];
        }
        let bend = if horizontal { Coordinate { x: target.x, y: last.y } } else { Coordinate { x: last.x, y: target.y } };
        vec![bend, target]
    }
    fn confirm(&mut self, target: Coordinate, horizontal: bool) -> Result<(), String> {
        let additions = self.additions(target, horizontal);
        if self.points.len() + additions.len() > 256 {
            return Err("At most256 confirmed vertices; finish or backspace".into());
        }
        if !additions.is_empty() {
            self.clicks.push(self.points.len());
            self.points.extend(additions);
        }
        Ok(())
    }
}
fn world(at: Coordinate) -> WorldPos {
    WorldPos::new(at.x.to_mm(), at.y.to_mm())
}
struct Pending {
    stream: ObjectId,
    sheet: ObjectId,
    cancelled: bool,
    receiver: Receiver<Result<WireDraft, String>>,
}
pub struct SchematicWiresUi {
    draft: Option<WireDraft>,
    pending: Option<Pending>,
    message: Option<String>,
    grid: i64,
    horizontal: bool,
}
impl Default for SchematicWiresUi {
    fn default() -> Self {
        Self { draft: None, pending: None, message: None, grid: 1_000_000, horizontal: true }
    }
}
impl SchematicWiresUi {
    pub fn active(&self) -> bool {
        self.draft.is_some() || self.pending.as_ref().is_some_and(|p| !p.cancelled)
    }
    pub fn confirmed_vertices(&self) -> Vec<Coordinate> {
        self.draft.as_ref().map_or_else(Vec::new, |d| d.points.clone())
    }
    pub fn cancel(&mut self) {
        self.draft = None;
        self.message = None;
        if let Some(pending) = &mut self.pending {
            pending.cancelled = true;
        }
    }
    fn poll(&mut self, context: Option<(ObjectId, EventCursor, ObjectId)>) {
        if self.draft.as_ref().is_some_and(|d| {
            context.is_none_or(|(p, c, s)| {
                p != d.snapshot.expected_project || c.stream_id != d.snapshot.source_cursor.stream_id || s != d.snapshot.expected.id
            })
        }) {
            self.cancel();
        }
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(value) => value,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Wire snapshot worker stopped; restart drawing".into()),
        };
        let pending = self.pending.take().expect("completed worker");
        if pending.cancelled || context.is_none_or(|(_, c, s)| c.stream_id != pending.stream || s != pending.sheet) {
            return;
        }
        match result {
            Ok(draft) if context == Some((draft.snapshot.expected_project, draft.snapshot.source_cursor, draft.snapshot.expected.id)) => {
                self.draft = Some(draft)
            }
            Ok(_) => self.message = Some("Document changed while preparing wires; restart drawing".into()),
            Err(error) => self.message = Some(error),
        }
    }
    fn start(&mut self, engine: EngineHandle, sheet_id: ObjectId) {
        if self.pending.is_some() {
            return;
        }
        self.cancel();
        let stream = engine.event_cursor().stream_id;
        let (sender, receiver) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-schematic-wires".into()).spawn(move || {
            let result =
                engine.snapshot_schematic_wire_sheet(&SchematicWireSheetQuery { sheet_id }).map_err(|e| e.to_string()).and_then(WireDraft::new);
            let _ = sender.try_send(result);
        }) {
            Ok(_) => self.pending = Some(Pending { stream, sheet: sheet_id, cancelled: false, receiver }),
            Err(error) => self.message = Some(format!("Could not start wire snapshot worker: {error}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_schematic_wires(&mut self) {
        if !self.schematic.active
            || self.schematic_object_draft_active()
            || self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
        {
            self.schematic_wires.message = Some("Finish or cancel the current object/file/board edit before drawing wires".into());
            return;
        }
        if let (Some(design), Some(sheet)) = (&self.design, self.schematic.active_sheet_id()) {
            self.schematic_wires.start(design.engine.clone(), sheet);
        }
    }
    pub(crate) fn schematic_wire_toolbar(&mut self, ui: &mut Ui) {
        let context = self.design.as_ref().and_then(|d| {
            self.schematic
                .active
                .then_some(d)
                .and_then(|d| self.schematic.active_sheet_id().map(|s| (d.engine.read(|p| p.id), d.engine.event_cursor(), s)))
        });
        self.schematic_wires.poll(context);
        if self.schematic_wires.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
            ui.label("Preparing bounded wire attachment snapshot…");
            if ui.button("Cancel wire preparation").clicked() {
                self.schematic_wires.cancel();
            }
        }
        if ui.button("Draw wires on canvas").clicked() {
            self.begin_schematic_wires();
        }
        if self.schematic_wires.draft.is_some() {
            let mut finish = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                finish = ui.button("Finish wires").clicked();
                cancel = ui.button("Cancel wires").clicked();
            });
            egui::ComboBox::from_id_salt("wire_grid").selected_text(format!("Grid {} mm", self.schematic_wires.grid as f64 / 1_000_000.0)).show_ui(
                ui,
                |ui| {
                    for (n, label) in [(100_000, "0.1 mm"), (250_000, "0.25 mm"), (500_000, "0.5 mm"), (1_000_000, "1 mm"), (1_270_000, "1.27 mm")] {
                        ui.selectable_value(&mut self.schematic_wires.grid, n, label);
                    }
                },
            );
            ui.checkbox(&mut self.schematic_wires.horizontal, "Horizontal first");
            ui.label("Click targets · Enter finish · Backspace last click · Esc cancel");
            ui.label("Exact anchors before grid. Interior crossings do not join; endpoints on wires join.");
            if let Some(d) = &self.schematic_wires.draft {
                ui.label(format!("{} confirmed vertices", d.points.len()));
                if context.is_none_or(|(_, cursor, _)| cursor != d.snapshot.source_cursor) {
                    ui.label("Wire preview is stale; cancel and restart before committing.");
                }
            }
            if finish {
                self.finish_schematic_wires();
            }
            if cancel {
                self.schematic_wires.cancel();
            }
        }
        if let Some(message) = &self.schematic_wires.message {
            ui.label(message);
        }
    }
    fn finish_schematic_wires(&mut self) {
        if self.files.busy() || self.schematic_object_draft_active() {
            self.schematic_wires.message = Some("Finish the active file/object edit before committing wires".into());
            return;
        }
        let Some(d) = &self.schematic_wires.draft else { return };
        let Some(design) = &self.design else { return };
        let command = DrawSchematicWirePolyline {
            expected_project: d.snapshot.expected_project,
            source_cursor: d.snapshot.source_cursor,
            expected: d.snapshot.expected.clone(),
            vertices: d.points.clone(),
        };
        match UiAction::DrawSchematicWirePolyline.run(&design.engine, serde_json::to_value(command).expect("wire command")) {
            Ok(_) => {
                self.schematic_wires.cancel();
                self.schematic_wires.message = Some("Wires created in one transaction; Undo removes the entire path".into());
            }
            Err(error) => self.schematic_wires.message = Some(format!("{error}; local preview retained")),
        }
    }
    pub(crate) fn schematic_wire_canvas(&mut self, ui: &mut Ui, response: &Response, painter: &egui::Painter) -> bool {
        if self.schematic_wires.draft.is_none() {
            return false;
        }
        let keyboard = response.hovered() && !ui.ctx().wants_keyboard_input();
        if keyboard && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.schematic_wires.cancel();
            return true;
        }
        if keyboard && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.finish_schematic_wires();
            return true;
        }
        let d = self.schematic_wires.draft.as_mut().expect("active draft");
        if keyboard
            && ui.input(|i| i.key_pressed(Key::Backspace))
            && let Some(len) = d.clicks.pop()
        {
            d.points.truncate(len);
        }
        let camera = self.schematic.camera;
        let target = response.hover_pos().map(|pointer| d.target(camera, response.rect, pointer, self.schematic_wires.grid));
        if response.clicked_by(PointerButton::Primary) && !ui.input(|i| i.key_down(Key::Space)) {
            match target.as_ref() {
                Some(Ok((target, _))) => {
                    if let Err(error) = d.confirm(*target, self.schematic_wires.horizontal) {
                        self.schematic_wires.message = Some(error);
                    }
                }
                Some(Err(error)) => self.schematic_wires.message = Some(error.clone()),
                None => {}
            }
        }
        let mut points = d.points.clone();
        if let Some(Ok((target, attached))) = &target {
            points.extend(d.additions(*target, self.schematic_wires.horizontal));
            painter.circle_stroke(
                camera.world_to_screen(world(*target), response.rect),
                if *attached { 7.0 } else { 4.0 },
                Stroke::new(2.0, Color32::from_rgb(255, 180, 70)),
            );
        }
        for pair in points.windows(2) {
            painter.line_segment(
                [camera.world_to_screen(world(pair[0]), response.rect), camera.world_to_screen(world(pair[1]), response.rect)],
                Stroke::new(2.5, Color32::from_rgb(255, 180, 70)),
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viamind_doc::{Project, Sheet};
    fn snapshot() -> SchematicWireSnapshot {
        let mut p = Project::new("worker");
        let s = Sheet::new("Main");
        let sheet_id = s.id;
        p.schematic.sheets.insert(s.id, s);
        EngineHandle::new(p).snapshot_schematic_wire_sheet(&SchematicWireSheetQuery { sheet_id }).unwrap()
    }
    #[test]
    fn retained_worker_cancel_and_replacement_discard_completion() {
        let old = snapshot();
        let new = snapshot();
        let (sender, receiver) = sync_channel(1);
        let mut state = SchematicWiresUi {
            pending: Some(Pending { stream: old.source_cursor.stream_id, sheet: old.expected.id, cancelled: false, receiver }),
            ..Default::default()
        };
        state.cancel();
        assert!(state.pending.is_some());
        sender.try_send(Ok(WireDraft::new(old).unwrap())).unwrap();
        state.poll(Some((new.expected_project, new.source_cursor, new.expected.id)));
        assert!(state.draft.is_none());
        assert!(state.pending.is_none());
    }
    #[test]
    fn same_context_cancel_and_changed_sequence_never_publish() {
        for cancelled in [true, false] {
            let old = snapshot();
            let (sender, receiver) = sync_channel(1);
            let mut state = SchematicWiresUi {
                pending: Some(Pending { stream: old.source_cursor.stream_id, sheet: old.expected.id, cancelled, receiver }),
                ..Default::default()
            };
            let mut cursor = old.source_cursor;
            if !cancelled {
                cursor.sequence += 1;
            }
            let context = Some((old.expected_project, cursor, old.expected.id));
            sender.try_send(Ok(WireDraft::new(old).unwrap())).unwrap();
            state.poll(context);
            assert!(state.draft.is_none());
            assert!(state.pending.is_none());
        }
    }
    #[test]
    fn path_budget_and_exact_anchor_priority_over_grid() {
        let mut d = WireDraft::new(snapshot()).unwrap();
        for x in 0..256 {
            d.confirm(Coordinate { x: Nm(x), y: Nm(0) }, true).unwrap();
        }
        let before = d.points.clone();
        assert!(d.confirm(Coordinate { x: Nm(256), y: Nm(0) }, true).is_err());
        assert_eq!(d.points, before);
        let at = Coordinate { x: Nm(1_000_001), y: Nm(2_000_003) };
        d.anchors = vec![Anchor { id: ObjectId::new(), kind: 0, at }];
        let p = Point::new(at.x, at.y);
        d.index = SpatialIndex::bulk_load([(0, Rect::new(p, p))]);
        let camera = Camera::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        let pointer = camera.world_to_screen(world(at), viewport);
        assert_eq!(d.target(camera, viewport, pointer, 1_000_000).unwrap(), (at, true));
    }
}
