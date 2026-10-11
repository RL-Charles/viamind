//! Local snapped route preview; only the shared guarded command commits copper.
use crate::{
    ViamindApp,
    actions::UiAction,
    camera::{Camera, WorldPos},
};
use crate::{layers::LayerStack, pad_routing::PadDraft};
use egui::{Color32, Key, PointerButton, Response, Stroke, Ui};
use std::time::Instant;
use viamind_doc::CopperLayer;
use viamind_engine::{EngineHandle, PadEndpointId, RoutePolyline};
use viamind_geom::{Nm, Point};
use viamind_router::CornerMode;

#[derive(Debug)]
struct Settings {
    net: String,
    width: String,
    grid: String,
    clearance: String,
    edge: String,
    layer: CopperLayer,
    pad_mode: bool,
    via_mode: bool,
    via_diameter: String,
    via_drill: String,
    corner_mode: Option<CornerMode>,
    walkaround: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            net: "GND".into(),
            width: "0.25".into(),
            grid: "0.25".into(),
            clearance: "0.2".into(),
            edge: "0.25".into(),
            layer: CopperLayer::Top,
            pad_mode: false,
            via_mode: false,
            via_diameter: "0.6".into(),
            via_drill: "0.3".into(),
            corner_mode: None,
            walkaround: false,
        }
    }
}
#[derive(Debug)]
struct Draft {
    input: RoutePolyline,
    hover: Option<Point>,
}
#[derive(Debug)]
struct WalkaroundWorker {
    token: viamind_doc::ObjectId,
    input: viamind_engine::RoutePadWalkaround,
    receiver: std::sync::mpsc::Receiver<Result<viamind_engine::CheckedWalkaround, String>>,
}
#[derive(Debug, Default)]
pub struct RoutingUi {
    settings: Option<Settings>,
    draft: Option<Draft>,
    pad_draft: Option<PadDraft>,
    pub error: Option<String>,
    pub last_commit_ms: Option<f64>,
    walkaround_worker: Option<WalkaroundWorker>,
}
impl RoutingUi {
    pub(crate) fn sync_walkaround(&mut self, ctx: &egui::Context, engine: &EngineHandle) {
        let Some(WalkaroundWorker { token, input, receiver }) = &self.walkaround_worker else { return };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("Walkaround worker stopped".into()),
        };
        if let Some(draft) = &mut self.pad_draft {
            draft.plan_pending = false;
            if draft.token == *token && draft.walkaround_input().as_ref() == Some(input) && engine.event_cursor() == input.expected_cursor {
                match result {
                    Ok(plan) => {
                        draft.checked = Some(plan);
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        self.walkaround_worker = None;
    }
    pub fn editing(&self) -> bool {
        self.settings.is_some() || self.draft.is_some() || self.pad_draft.is_some()
    }
    pub fn previewing(&self) -> bool {
        self.draft.is_some() || self.pad_draft.is_some()
    }
    pub fn points(&self) -> &[Point] {
        self.draft.as_ref().map_or(&[], |d| d.input.points.as_slice())
    }
    pub fn hover(&self) -> Option<Point> {
        self.draft.as_ref().and_then(|d| d.hover)
    }
    pub fn pad_endpoints(&self) -> (Option<PadEndpointId>, Option<PadEndpointId>) {
        self.pad_draft.as_ref().map_or((None, None), PadDraft::endpoints)
    }
    pub fn pad_planned_points(&self) -> Vec<Point> {
        self.pad_draft.as_ref().and_then(|d| d.planned_path().ok().flatten()).map_or_else(Vec::new, |plan| plan.points)
    }
    pub fn pad_via(&self) -> Option<Point> {
        self.pad_draft.as_ref().and_then(PadDraft::via)
    }
    pub fn pad_after(&self) -> &[Point] {
        self.pad_draft.as_ref().map_or(&[], PadDraft::after)
    }
    pub fn pad_interior(&self) -> &[Point] {
        self.pad_draft.as_ref().map_or(&[], PadDraft::interior)
    }
    pub fn cancel(&mut self) {
        self.settings = None;
        self.draft = None;
        self.pad_draft = None;
    }
    fn finish_pad(&mut self, engine: &EngineHandle) {
        let Some(draft) = &self.pad_draft else { return };
        match draft.finish(engine) {
            Ok(ms) => {
                self.last_commit_ms = Some(ms);
                self.pad_draft = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn finish(&mut self, engine: &EngineHandle) {
        let Some(draft) = &self.draft else { return };
        if draft.input.points.len() < 2 {
            self.error = Some("Place at least two route vertices before finishing".into());
            return;
        }
        let start = Instant::now();
        let result = UiAction::RoutePolyline.run(engine, serde_json::to_value(&draft.input).expect("route serializes"));
        self.last_commit_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        match result {
            Ok(_) => {
                self.draft = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(crate) fn panel(&mut self, ui: &mut Ui, engine: &EngineHandle) {
        if self.draft.is_some() {
            ui.separator();
            ui.heading("PCB route preview");
            let draft = self.draft.as_ref().unwrap();
            ui.label(format!("{} · {:?} · {:.6} mm wide", draft.input.net, draft.input.layer, draft.input.width.to_mm()));
            ui.label("Click: vertex · Backspace: remove · Enter: finish · Escape: cancel");
            ui.label("Orange preview is unchecked until commit.");
            ui.label(format!("{} fixed vertices", draft.input.points.len()));
            if let Some(point) = draft.hover {
                ui.monospace(format!("Snap X {:.6} Y {:.6} mm", point.x.to_mm(), point.y.to_mm()));
            }
            if engine.event_cursor() != draft.input.expected_cursor {
                ui.colored_label(Color32::YELLOW, "Session changed; cancel and restart this route.");
            }
            let mut finish = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                finish = ui.add_enabled(draft.input.points.len() >= 2, egui::Button::new("Finish route")).clicked();
                cancel = ui.button("Cancel route").clicked();
            });
            if cancel {
                self.cancel();
            } else if finish {
                self.finish(engine);
            }
        }
        if let Some(draft) = &mut self.pad_draft {
            draft.plan_pending = self.walkaround_worker.is_some();
            let (finish, cancel) = draft.panel(ui, engine);
            if draft.plan_requested {
                draft.plan_requested = false;
                if self.walkaround_worker.is_none()
                    && let Some(input) = draft.walkaround_input()
                {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let captured = input.clone();
                    let engine = engine.clone();
                    let ctx = ui.ctx().clone();
                    let token = draft.token;
                    match std::thread::Builder::new().name("walkaround-plan".into()).spawn(move || {
                        let result = engine.plan_pad_walkaround(&captured).map_err(|e| e.to_string());
                        let _ = sender.send(result);
                        ctx.request_repaint();
                    }) {
                        Ok(_) => {
                            draft.plan_pending = true;
                            self.walkaround_worker = Some(WalkaroundWorker { token, input, receiver });
                        }
                        Err(error) => self.error = Some(format!("Cannot start walkaround worker: {error}")),
                    }
                }
            }
            if cancel {
                self.cancel();
            } else if finish {
                self.finish_pad(engine);
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        if let Some(ms) = self.last_commit_ms {
            ui.small(format!("Last route commit check: {ms:.2} ms"));
        }
    }
    pub(crate) fn canvas(&mut self, ui: &Ui, response: &Response, camera: &Camera, engine: &EngineHandle, layers: &LayerStack, enabled: bool) {
        if let Some(draft) = &mut self.pad_draft {
            let (cancel, finish) = draft.canvas(ui, response, camera, engine, layers, enabled, &mut self.error);
            if cancel {
                self.cancel();
            } else if finish {
                self.finish_pad(engine);
            }
            return;
        }
        let Some(draft) = &mut self.draft else { return };
        let lost = ui.input(|i| !i.focused || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))));
        if !enabled || lost || engine.read(|p| p.id) != draft.input.expected_project {
            self.cancel();
            return;
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.cancel();
            return;
        }
        let keyboard = !ui.ctx().egui_wants_keyboard_input();
        if keyboard && ui.input(|i| i.key_pressed(Key::Backspace)) {
            draft.input.points.pop();
            self.error = None;
        }
        let space = ui.input(|i| i.key_down(Key::Space));
        draft.hover = response.hover_pos().and_then(|position| {
            let world = camera.screen_to_world(position, response.rect);
            if !world.x.is_finite() || !world.y.is_finite() || world.x.abs() > 10000.0 || world.y.abs() > 10000.0 {
                return None;
            }
            viamind_router::snap(Point::from_mm(world.x, world.y), draft.input.grid).ok()
        });
        if !space
            && response.clicked_by(PointerButton::Primary)
            && let Some(point) = draft.hover
            && draft.input.points.last() != Some(&point)
            && draft.input.points.len() < 1025
        {
            draft.input.points.push(point);
            self.error = None;
        }
        let painter = ui.painter().with_clip_rect(response.rect);
        let screen = |point: Point| camera.world_to_screen(WorldPos::new(point.x.to_mm(), point.y.to_mm()), response.rect);
        let stroke = Stroke::new(((draft.input.width.to_mm() * camera.zoom) as f32).max(1.0), Color32::from_rgb(255, 174, 64));
        for pair in draft.input.points.windows(2) {
            painter.line_segment([screen(pair[0]), screen(pair[1])], stroke);
        }
        if let (Some(&last), Some(hover)) = (draft.input.points.last(), draft.hover) {
            painter.line_segment([screen(last), screen(hover)], stroke);
        }
        for &point in &draft.input.points {
            painter.circle_stroke(screen(point), 4.0, Stroke::new(1.0, Color32::WHITE));
        }
        if let Some(point) = draft.hover {
            painter.circle_stroke(screen(point), 5.0, Stroke::new(1.0, Color32::YELLOW));
        }
        if keyboard && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.finish(engine);
        }
    }
}
fn mm(text: &str, positive: bool) -> Result<Nm, String> {
    let value = text.trim().parse::<f64>().map_err(|_| "Enter a decimal millimetre value".to_owned())?;
    if !value.is_finite() || value < 0.0 || value > 100.0 {
        return Err("Routing sizes must be finite within 0..100 mm".into());
    }
    let nm = Nm::from_mm(value);
    if positive && nm.0 <= 0 {
        return Err("Track width and grid must be at least one nanometre".into());
    }
    Ok(nm)
}
impl ViamindApp {
    pub fn begin_walkaround_route(&mut self) {
        if self.routing.editing() {
            return;
        }
        self.begin_pad_route();
        if let Some(settings) = &mut self.routing.settings {
            settings.walkaround = true;
        }
    }
    pub fn begin_posture_route(&mut self, mode: CornerMode) {
        if self.routing.editing() {
            return;
        }
        self.begin_pad_route();
        if let Some(settings) = &mut self.routing.settings {
            settings.corner_mode = Some(mode);
        }
    }
    pub fn begin_via_route(&mut self) {
        if self.routing.editing() {
            return;
        }
        self.begin_pad_route();
        if let Some(settings) = &mut self.routing.settings {
            settings.via_mode = true;
        }
    }

    pub fn begin_pad_route(&mut self) {
        if self.routing.editing() {
            return;
        }
        self.begin_route();
        if let Some(settings) = &mut self.routing.settings {
            settings.pad_mode = true;
        }
    }
    pub fn begin_route(&mut self) {
        if self.schematic.active
            || self.files.busy()
            || self.parts.placing()
            || self.pads.editing()
            || self.outlines.editing()
            || self.copper.editing()
            || self.routing.editing()
        {
            return;
        }
        let Some(design) = &self.design else { return };
        if design.engine.read(|p| p.board.outline.is_none()) {
            self.routing.error = Some("Set an explicit Board outline before routing".into());
            return;
        }
        self.selection.cancel_gestures();
        self.routing.error = None;
        self.routing.settings = Some(Settings::default());
    }
    pub(crate) fn route_dialog(&mut self, ui: &mut Ui) {
        let Some(settings) = &mut self.routing.settings else { return };
        let mut start = false;
        let mut cancel = false;
        let modal=egui::Modal::new(egui::Id::new("route_settings")).show(ui.ctx(),|ui|{
            ui.heading(if settings.via_mode {"Route pads through via"} else if settings.pad_mode {"Route pad connection"} else {"Route PCB polyline"});
            egui::Grid::new("route_fields").num_columns(2).show(ui,|ui|{
                if !settings.pad_mode {let label=ui.label("Route net");ui.add(egui::TextEdit::singleline(&mut settings.net).id_salt("Route net")).labelled_by(label.id);ui.end_row();}
                for (label,value) in [("Route width (mm)",&mut settings.width),("Route grid (mm)",&mut settings.grid),("Route copper clearance (mm)",&mut settings.clearance),("Route edge clearance (mm)",&mut settings.edge)] {
                    let label_response=ui.label(label);ui.add(egui::TextEdit::singleline(value).id_salt(label)).labelled_by(label_response.id);ui.end_row();
                }
                if settings.via_mode {for (label,value) in [("Via copper diameter (mm)",&mut settings.via_diameter),("Via drill diameter (mm)",&mut settings.via_drill)] {let label_response=ui.label(label);ui.add(egui::TextEdit::singleline(value).id_salt(label)).labelled_by(label_response.id);ui.end_row();}}
                if !settings.pad_mode {ui.label("Route layer");ui.horizontal(|ui|{ui.radio_value(&mut settings.layer,CopperLayer::Top,"Top copper");ui.radio_value(&mut settings.layer,CopperLayer::Bottom,"Bottom copper");});ui.end_row();}
            });
            if settings.pad_mode && !settings.via_mode {
                ui.checkbox(&mut settings.walkaround, "Static cardinal walkaround");
                ui.add_enabled_ui(!settings.walkaround, |ui| {
                    ui.radio_value(&mut settings.corner_mode, None, "Manual segments");
                    ui.radio_value(&mut settings.corner_mode, Some(CornerMode::Cardinal), "Cardinal corners");
                    ui.radio_value(&mut settings.corner_mode, Some(CornerMode::Octilinear), "45-degree corners");
                });
                if settings.corner_mode.is_some() && !settings.walkaround {ui.label("Generated corners follow the selected posture. Exact off-grid pad attachments use short arbitrary-angle stubs, checked with the whole route.");}
            }
            ui.label("Commit checks explicit pads, tracks, through vias and outline/cutouts. Parts without explicit pads and other scoped DRC exclusions remain unchecked.");
            ui.label(if settings.via_mode {"Choose opposite-side same-net explicit pads and one snapped through via. Both legs and via commit atomically after scoped checks. No automatic obstacle avoidance or electrical continuity verification."} else if settings.pad_mode {"Pick explicit pad identities or a connection guide. Quarter-turn transforms only; exact off-grid pad centers, snapped interior vertices. Conservative stored-endpoint copper proofs may suppress guides; no physical continuity certification."} else {"Preview is local. No push/shove, obstacle avoidance, automatic pad attachment or electrical continuity verification."});
            if let Some(error)=&self.routing.error {ui.colored_label(Color32::LIGHT_RED,error);}
            ui.horizontal(|ui|{start=ui.button("Start route").clicked();cancel=ui.button("Cancel route settings").clicked();});
        });
        if cancel || modal.should_close() {
            self.routing.cancel();
            return;
        }
        if start {
            let pad_mode = settings.pad_mode;
            let corner_mode = settings.corner_mode;
            let walkaround = settings.walkaround;
            let result = (|| {
                let width = mm(&settings.width, true)?;
                let grid = mm(&settings.grid, true)?;
                let rules = viamind_drc::DrcInput {
                    copper_clearance_mm: mm(&settings.clearance, false)?.to_mm(),
                    edge_clearance_mm: mm(&settings.edge, false)?.to_mm(),
                    ..Default::default()
                };
                let net = settings.net.trim().to_owned();
                if net.is_empty() {
                    return Err("Enter a nonempty route net".into());
                }
                let (project, cursor) =
                    self.design.as_ref().ok_or("Open a project before routing")?.engine.with_engine(|e| (e.project().id, e.event_cursor()));
                let via_sizes = if settings.via_mode {
                    let diameter = mm(&settings.via_diameter, true)?;
                    let drill = mm(&settings.via_drill, true)?;
                    if drill >= diameter {
                        return Err("Via drill must be smaller than copper diameter".into());
                    }
                    Some((diameter, drill))
                } else {
                    None
                };
                Ok((
                    RoutePolyline {
                        expected_project: project,
                        expected_cursor: cursor,
                        points: Vec::new(),
                        width,
                        grid,
                        layer: settings.layer,
                        net,
                        rules,
                    },
                    via_sizes,
                ))
            })();
            match result {
                Ok((input, via_sizes)) => {
                    if pad_mode {
                        match self.design.as_ref().unwrap().engine.route_endpoint_snapshot() {
                            Ok(snapshot) => {
                                let mut draft = PadDraft::new(snapshot, input.width, input.grid, input.rules);
                                if let Some((diameter, drill)) = via_sizes {
                                    draft = draft.with_via(diameter, drill);
                                } else if walkaround {
                                    draft = draft.with_walkaround();
                                } else if let Some(mode) = corner_mode {
                                    draft = draft.with_posture(mode);
                                }
                                self.routing.pad_draft = Some(draft);
                            }
                            Err(e) => {
                                self.routing.error = Some(e.to_string());
                                return;
                            }
                        }
                    } else {
                        self.routing.draft = Some(Draft { input, hover: None });
                    }
                    self.routing.settings = None;
                    self.routing.error = None;
                }
                Err(e) => self.routing.error = Some(e),
            }
        }
    }
}

#[cfg(test)]
mod walkaround_worker_tests {
    use super::*;
    #[test]
    fn cancellation_retains_worker_until_completion_and_discards_old_checked_plans() {
        let engine = EngineHandle::new(viamind_doc::Project::new("retained worker"));
        let id = PadEndpointId { part_id: viamind_doc::ObjectId::new(), pad_id: viamind_doc::ObjectId::new() };
        let input = viamind_engine::RoutePadWalkaround {
            expected_project: engine.read(|p| p.id),
            expected_cursor: engine.event_cursor(),
            start: id,
            target: id,
            grid: Nm(1),
            width: Nm(1),
            rules: Default::default(),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut state =
            RoutingUi { walkaround_worker: Some(WalkaroundWorker { token: viamind_doc::ObjectId::new(), input, receiver }), ..Default::default() };
        state.cancel();
        assert!(state.walkaround_worker.is_some());
        assert!(!state.editing());
        state.sync_walkaround(&egui::Context::default(), &engine);
        assert!(state.walkaround_worker.is_some());
        sender
            .send(Ok(viamind_engine::CheckedWalkaround {
                project_id: engine.read(|p| p.id),
                source_cursor: engine.event_cursor(),
                layer: CopperLayer::Top,
                net: "N".into(),
                plan: viamind_router::WalkaroundPlan {
                    path: viamind_router::PadPathPlan { points: vec![Point::ORIGIN, Point::new(Nm(1), Nm(0))], endpoint_stub_segments: 0 },
                    waypoints: Vec::new(),
                    work: Default::default(),
                },
                complete_scoped_preflight: true,
            }))
            .unwrap();
        state.sync_walkaround(&egui::Context::default(), &engine);
        assert!(state.walkaround_worker.is_none());
        assert!(state.error.is_none());
    }
}
