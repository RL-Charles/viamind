//! Local exact terminal draft. Only a shared guarded engine command adds copper.
use crate::{
    actions::UiAction,
    camera::{Camera, WorldPos},
    layers::{LayerId, LayerStack},
};
use egui::{Color32, Key, PointerButton, Response, Stroke, Ui};
use std::time::Instant;
use viamind_doc::CopperLayer;
use viamind_engine::{EngineHandle, RouteTerminalId, RouteTerminalPolyline, RouteTerminalPosture, RouteTerminalSnapshot};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};
use viamind_router::{CornerMode, Posture};
#[derive(Clone, Copy, Debug, PartialEq)]
enum Picking {
    Start,
    Target,
    Interior,
}
#[derive(Debug)]
pub(crate) struct TerminalDraft {
    snapshot: RouteTerminalSnapshot,
    index: SpatialIndex<usize>,
    layer: CopperLayer,
    width: Nm,
    grid: Nm,
    rules: viamind_drc::DrcInput,
    mode: Option<CornerMode>,
    posture: Posture,
    picking: Picking,
    start: Option<usize>,
    target: Option<usize>,
    interior: Vec<Point>,
    hover: Option<Point>,
    hovered: Option<usize>,
}
impl TerminalDraft {
    pub(crate) fn new(
        mut snapshot: RouteTerminalSnapshot,
        layer: CopperLayer,
        width: Nm,
        grid: Nm,
        rules: viamind_drc::DrcInput,
        mode: Option<CornerMode>,
    ) -> Self {
        snapshot.terminals.retain(|t| t.layer == layer);
        let index = SpatialIndex::bulk_load(snapshot.terminals.iter().enumerate().filter_map(|(i, t)| t.center_nm.map(|p| (i, Rect::new(p, p)))));
        Self {
            snapshot,
            index,
            layer,
            width,
            grid,
            rules,
            mode,
            posture: Posture::Primary,
            picking: Picking::Start,
            start: None,
            target: None,
            interior: Vec::new(),
            hover: None,
            hovered: None,
        }
    }
    pub(crate) fn endpoints(&self) -> (Option<RouteTerminalId>, Option<RouteTerminalId>) {
        (self.start.map(|i| self.snapshot.terminals[i].id), self.target.map(|i| self.snapshot.terminals[i].id))
    }
    pub(crate) fn interior(&self) -> &[Point] {
        &self.interior
    }
    pub(crate) fn points(&self) -> Result<Vec<Point>, String> {
        let Some(a) = self.start.and_then(|i| self.snapshot.terminals[i].center_nm) else { return Ok(Vec::new()) };
        let Some(b) = self.target.and_then(|i| self.snapshot.terminals[i].center_nm) else { return Ok(vec![a]) };
        if let Some(mode) = self.mode {
            viamind_router::plan_pad_path(a, b, &self.interior, self.grid, mode, self.posture).map(|p| p.points).map_err(str::to_owned)
        } else {
            let mut points = Vec::with_capacity(self.interior.len() + 2);
            points.push(a);
            points.extend_from_slice(&self.interior);
            points.push(b);
            Ok(points)
        }
    }
    pub(crate) fn finish(&self, engine: &EngineHandle) -> Result<f64, String> {
        let (Some(start), Some(target)) = self.endpoints() else { return Err("Choose both existing terminals before finishing".into()) };
        let now = Instant::now();
        let result = if let Some(mode) = self.mode {
            let input = RouteTerminalPosture {
                expected_project: self.snapshot.project_id,
                expected_cursor: self.snapshot.source_cursor,
                start,
                target,
                layer: self.layer,
                waypoints: self.interior.clone(),
                grid: self.grid,
                width: self.width,
                mode,
                posture: self.posture,
                rules: self.rules.clone(),
            };
            UiAction::RouteTerminalPosture.run(engine, serde_json::to_value(input).expect("terminal posture serializes"))
        } else {
            let input = RouteTerminalPolyline {
                expected_project: self.snapshot.project_id,
                expected_cursor: self.snapshot.source_cursor,
                start,
                target,
                layer: self.layer,
                interior: self.interior.clone(),
                grid: self.grid,
                width: self.width,
                rules: self.rules.clone(),
            };
            UiAction::RouteTerminalPolyline.run(engine, serde_json::to_value(input).expect("terminal polyline serializes"))
        };
        result.map(|_| now.elapsed().as_secs_f64() * 1000.0).map_err(|e| e.to_string())
    }
    pub(crate) fn window(&mut self, ui: &Ui, engine: &EngineHandle, error: Option<&str>) -> (bool, bool) {
        let mut finish = false;
        let mut cancel = false;
        let mut open = true;
        let rect = ui.ctx().content_rect();
        let current = engine.event_cursor() == self.snapshot.source_cursor;
        egui::Window::new("Existing copper route").open(&mut open).collapsible(false).resizable(false)
   .anchor(egui::Align2::RIGHT_TOP,[-12.0,42.0]).default_width(360.0).max_width((rect.width()-32.0).max(180.0)).show(ui.ctx(),|ui| {
    egui::ScrollArea::vertical().id_salt("terminal_route_details").max_height((rect.height()-155.0).clamp(70.0,260.0)).show(ui,|ui| {
     ui.label(format!("{:?} copper; explicit width {}nm; grid {}nm",self.layer,self.width.0,self.grid.0));
     ui.label(match self.picking {Picking::Start=>"Click an existing pad, track end or via center to start.",Picking::Target=>"Click a distinct same-net terminal on this layer to finish attachment.",Picking::Interior=>"Click: grid vertex; Backspace: remove; Enter: finish; Escape: cancel."});
     for(label,index)in [("Start",self.start),("Target",self.target)] {
      if let Some(i)=index {let row=&self.snapshot.terminals[i];ui.label(format!("{label}: {} / {}",row.label,row.net.as_deref().unwrap_or("unassigned")));if let Some(p)=row.center_nm{ui.monospace(format!("Exact X {} Y {}nm",p.x.0,p.y.0));}}
     }
     if let Some(mode)=self.mode {ui.label(format!("{:?}: {}",mode,self.posture.label(mode)));if ui.button("Switch terminal posture").clicked(){self.posture.toggle();}}
     ui.horizontal_wrapped(|ui|{if ui.button("Choose start terminal").clicked(){self.picking=Picking::Start;}
     if ui.button("Choose target terminal").clicked(){self.picking=Picking::Target;}});
     if let Some(i)=self.hovered {let row=&self.snapshot.terminals[i];ui.label(format!("Hover: {} / {}",row.label,row.net.as_deref().unwrap_or("unassigned")));}
     let unsupported=self.snapshot.terminals.iter().filter(|t|t.unsupported_reason.is_some()).count();
     ui.label(format!("{} interior vertices; {} unsupported terminals; {} legacy parts omitted",self.interior.len(),unsupported,self.snapshot.omitted_legacy_parts));
     if unsupported>0 {ui.collapsing("Unsupported terminal reasons",|ui|{for t in self.snapshot.terminals.iter().filter(|t|t.unsupported_reason.is_some()).take(8){ui.label(format!("{}: {}",t.label,t.unsupported_reason.as_deref().unwrap_or_default()));}ui.label("Complete details are available through query_route_terminals.");});}
     ui.label("Orange geometry is unchecked until commit. Existing copper is not resized. No midsegment split, new via, shove or physical continuity certification.");
     if !current {ui.colored_label(Color32::YELLOW,"Session changed; cancel and restart.");}
     if let Err(e)=self.points(){ui.colored_label(Color32::YELLOW,e);}
     if let Some(e)=error{ui.colored_label(Color32::LIGHT_RED,e);}
    });
    ui.separator();
    ui.label("Unchecked local preview; Finish runs scoped checks.");
    ui.horizontal(|ui|{finish=ui.add_enabled(current&&self.start.is_some()&&self.target.is_some()&&self.points().is_ok(),egui::Button::new("Finish terminal route")).clicked();cancel=ui.button("Cancel terminal route").clicked();});
   });
        (finish, cancel || !open)
    }
    fn hit(&self, p: egui::Pos2, camera: &Camera, viewport: egui::Rect) -> Option<usize> {
        let world = camera.screen_to_world(p, viewport);
        if !world.x.is_finite() || !world.y.is_finite() || camera.zoom <= 0.0 {
            return None;
        }
        let radius = (8.0 / camera.zoom * 1e6).ceil().clamp(1.0, 20_000_000_000.0) as i64;
        let c = Point::from_mm(world.x, world.y);
        let min = Point::new(Nm(c.x.0.saturating_sub(radius)), Nm(c.y.0.saturating_sub(radius)));
        let max = Point::new(Nm(c.x.0.saturating_add(radius)), Nm(c.y.0.saturating_add(radius)));
        self.index
            .query_rect(&Rect::new(min, max))
            .filter_map(|(i, _)| {
                let c = self.snapshot.terminals[i].center_nm?;
                let s = camera.world_to_screen(WorldPos::new(c.x.to_mm(), c.y.to_mm()), viewport);
                let distance = s.distance_sq(p);
                (distance <= 64.0).then_some((i, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|r| r.0)
    }
    pub(crate) fn canvas(
        &mut self,
        ui: &Ui,
        response: &Response,
        camera: &Camera,
        engine: &EngineHandle,
        layers: &LayerStack,
        enabled: bool,
        error: &mut Option<String>,
    ) -> (bool, bool) {
        let lost = ui.input(|i| !i.focused || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))));
        if !enabled
            || lost
            || engine.event_cursor() != self.snapshot.source_cursor
            || engine.read(|p| p.id) != self.snapshot.project_id
            || ui.input(|i| i.key_pressed(Key::Escape))
        {
            return (true, false);
        }
        let keyboard = !ui.ctx().egui_wants_keyboard_input();
        if keyboard && ui.input(|i| i.key_pressed(Key::Backspace)) {
            self.interior.pop();
            *error = None;
        }
        if keyboard && ui.input(|i| i.key_pressed(Key::Slash)) {
            self.posture.toggle();
        }
        let layer = match self.layer {
            CopperLayer::Top => LayerId::F_CU,
            CopperLayer::Bottom => LayerId::B_CU,
        };
        let visible = layers.get(layer).is_some_and(|l| l.visible && l.opacity > 0.0);
        self.hovered = if visible { response.hover_pos().and_then(|p| self.hit(p, camera, response.rect)) } else { None };
        self.hover = response.hover_pos().and_then(|p| {
            let w = camera.screen_to_world(p, response.rect);
            if !w.x.is_finite() || !w.y.is_finite() || w.x.abs() > 10000.0 || w.y.abs() > 10000.0 {
                return None;
            }
            viamind_router::snap(Point::from_mm(w.x, w.y), self.grid).ok()
        });
        if response.clicked_by(PointerButton::Primary) && !ui.input(|i| i.key_down(Key::Space)) {
            match self.picking {
                Picking::Start | Picking::Target => {
                    if let Some(i) = self.hovered {
                        let row = &self.snapshot.terminals[i];
                        if row.unsupported_reason.is_some() {
                            *error = row.unsupported_reason.clone();
                        } else if self.picking == Picking::Start {
                            self.start = Some(i);
                            self.target = None;
                            self.interior.clear();
                            self.picking = Picking::Target;
                            *error = None;
                        } else if let Some(a) = self.start {
                            let start = &self.snapshot.terminals[a];
                            if start.id == row.id || start.center_nm == row.center_nm || start.net != row.net {
                                *error = Some("Choose distinct positions with the exact same assigned net".into());
                            } else {
                                self.target = Some(i);
                                self.picking = Picking::Interior;
                                *error = None;
                            }
                        }
                    }
                }
                Picking::Interior => {
                    if let Some(p) = self.hover {
                        let maximum = if self.mode.is_some() { 510 } else { 1023 };
                        if self.interior.len() < maximum && self.interior.last() != Some(&p) {
                            self.interior.push(p);
                            *error = None;
                        }
                    }
                }
            }
        }
        if visible {
            let painter = ui.painter().with_clip_rect(response.rect);
            let screen = |p: Point| camera.world_to_screen(WorldPos::new(p.x.to_mm(), p.y.to_mm()), response.rect);
            let stroke = Stroke::new((self.width.to_mm() * camera.zoom).max(1.0) as f32, Color32::from_rgb(255, 174, 64));
            if let Ok(points) = self.points() {
                for pair in points.windows(2) {
                    painter.line_segment([screen(pair[0]), screen(pair[1])], stroke);
                }
                if self.target.is_none()
                    && let (Some(&a), Some(b)) = (points.first(), self.hover)
                {
                    painter.line_segment([screen(a), screen(b)], stroke);
                }
            }
            for i in [self.start, self.target, self.hovered].into_iter().flatten() {
                if let Some(p) = self.snapshot.terminals[i].center_nm {
                    painter.circle_stroke(screen(p), 7.0, Stroke::new(2.0, Color32::YELLOW));
                }
            }
            for &p in &self.interior {
                painter.circle_stroke(screen(p), 4.0, Stroke::new(1.0, Color32::WHITE));
            }
        }
        (false, keyboard && ui.input(|i| i.key_pressed(Key::Enter)))
    }
}
