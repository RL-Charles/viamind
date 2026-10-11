//! Frozen explicit-pad picking; the command resolves identities again at commit.
use crate::{
    actions::UiAction,
    camera::{Camera, WorldPos},
    layers::{LayerId, LayerStack},
};
use egui::{Color32, Key, PointerButton, Pos2, Response, Stroke, Ui};
use std::{collections::BTreeMap, time::Instant};
use viamind_doc::CopperLayer;
use viamind_engine::{EngineHandle, PadEndpointId, RouteEndpoint, RouteEndpointSnapshot, RoutePadPolyline};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picking {
    Start,
    Target,
    Interior,
}
#[derive(Debug)]
pub(crate) struct PadDraft {
    snapshot: RouteEndpointSnapshot,
    index: SpatialIndex<usize>,
    guide_index: SpatialIndex<usize>,
    guides: Vec<[usize; 2]>,
    picking: Picking,
    start: Option<usize>,
    target: Option<usize>,
    interior: Vec<Point>,
    hover: Option<Point>,
    hovered_pad: Option<usize>,
    width: Nm,
    grid: Nm,
    rules: viamind_drc::DrcInput,
}
fn world(p: Point) -> WorldPos {
    WorldPos::new(p.x.to_mm(), p.y.to_mm())
}
fn center(row: &RouteEndpoint) -> WorldPos {
    WorldPos::new(row.corners_mm.iter().map(|p| p[0]).sum::<f64>() / 4.0, row.corners_mm.iter().map(|p| p[1]).sum::<f64>() / 4.0)
}
fn visible(row: &RouteEndpoint, layers: &LayerStack) -> bool {
    let id = match row.layer {
        CopperLayer::Top => LayerId::F_CU,
        CopperLayer::Bottom => LayerId::B_CU,
    };
    layers.get(id).is_some_and(|l| l.visible && l.opacity > 0.0)
}
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}
fn inside(p: Pos2, corners: [Pos2; 4]) -> bool {
    // At distant zoom a nanometre pad can collapse in f32 screen coordinates.
    // Such geometry uses the circular origin tolerance, not an unbounded half-plane hit.
    let a = corners[1] - corners[0];
    let b = corners[3] - corners[0];
    if a.x * b.y - a.y * b.x == 0.0 {
        return false;
    }

    let mut positive = false;
    let mut negative = false;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        positive |= cross > 0.0;
        negative |= cross < 0.0;
    }
    !(positive && negative)
}
impl PadDraft {
    pub(crate) fn new(snapshot: RouteEndpointSnapshot, width: Nm, grid: Nm, rules: viamind_drc::DrcInput) -> Self {
        let index = SpatialIndex::bulk_load(snapshot.endpoints.iter().enumerate().map(|(i, row)| {
            // Outward rounding retains pad edges at the integer broad-phase boundary.
            let xs = row.corners_mm.map(|p| p[0]);
            let ys = row.corners_mm.map(|p| p[1]);
            let min = Point::new(
                Nm((xs.into_iter().fold(f64::INFINITY, f64::min) * 1e6).floor() as i64),
                Nm((ys.into_iter().fold(f64::INFINITY, f64::min) * 1e6).floor() as i64),
            );
            let max = Point::new(
                Nm((xs.into_iter().fold(f64::NEG_INFINITY, f64::max) * 1e6).ceil() as i64),
                Nm((ys.into_iter().fold(f64::NEG_INFINITY, f64::max) * 1e6).ceil() as i64),
            );
            (i, Rect::new(min, max))
        }));
        let lookup: BTreeMap<_, _> = snapshot.endpoints.iter().enumerate().map(|(i, row)| (row.id, i)).collect();
        let guides: Vec<_> = snapshot.guide_endpoints.iter().filter_map(|pair| Some([*lookup.get(&pair[0])?, *lookup.get(&pair[1])?])).collect();
        let guide_index = SpatialIndex::bulk_load(guides.iter().enumerate().map(|(i, pair)| {
            let a = center(&snapshot.endpoints[pair[0]]);
            let b = center(&snapshot.endpoints[pair[1]]);
            (i, Rect::new(Point::from_mm(a.x, a.y), Point::from_mm(b.x, b.y)))
        }));
        Self {
            snapshot,
            index,
            guide_index,
            guides,
            picking: Picking::Start,
            start: None,
            target: None,
            interior: Vec::new(),
            hover: None,
            hovered_pad: None,
            width,
            grid,
            rules,
        }
    }
    pub(crate) fn endpoints(&self) -> (Option<PadEndpointId>, Option<PadEndpointId>) {
        (self.start.map(|i| self.snapshot.endpoints[i].id), self.target.map(|i| self.snapshot.endpoints[i].id))
    }
    pub(crate) fn interior(&self) -> &[Point] {
        &self.interior
    }
    fn compatible(&self, a: usize, b: usize) -> Result<(), String> {
        let a = &self.snapshot.endpoints[a];
        let b = &self.snapshot.endpoints[b];
        if let Some(e) = &b.unsupported_reason {
            return Err(e.clone());
        }
        if a.id == b.id {
            return Err("Choose a different target pad".into());
        }
        if a.layer != b.layer || a.net != b.net {
            return Err("Choose a pad on the same copper side with the exact same assigned net".into());
        }
        if a.center_nm == b.center_nm {
            return Err("Endpoint centers coincide".into());
        }
        Ok(())
    }
    pub(crate) fn panel(&mut self, ui: &mut Ui, engine: &EngineHandle) -> (bool, bool) {
        ui.separator();
        ui.heading("Pad connection preview");
        let label = |id: Option<usize>| {
            id.map_or("Choose a pad".into(), |i| {
                let r = &self.snapshot.endpoints[i];
                format!("{} pad {} · {} · {:?}", r.designator, r.number, r.net.as_deref().unwrap_or("unassigned"), r.layer)
            })
        };
        ui.label(format!("Start: {}", label(self.start)));
        ui.label(format!("Target: {}", label(self.target)));
        match self.picking {
            Picking::Start => ui.label("Click a visible explicit pad or connection guide."),
            Picking::Target => ui.label("Click a compatible target pad."),
            Picking::Interior => ui.label("Click: insert grid vertex before target · Backspace: remove · Enter: finish"),
        };
        if let Some(i) = self.hovered_pad {
            let row = &self.snapshot.endpoints[i];
            ui.label(format!("Hover: {} pad {} · {} · {:?}", row.designator, row.number, row.net.as_deref().unwrap_or("unassigned"), row.layer));
            if let Some(p) = row.center_nm {
                ui.monospace(format!("Exact X {} Y {} nm", p.x.0, p.y.0));
            }
            if let Some(reason) = &row.unsupported_reason {
                ui.colored_label(Color32::YELLOW, reason);
            }
        }
        ui.label("Orange preview is unchecked. Guide counts remain conservative after routing.");
        ui.label(format!(
            "{} interior vertices · {} unsupported pads · {} legacy parts omitted",
            self.interior.len(),
            self.snapshot.unsupported_endpoints,
            self.snapshot.omitted_legacy_parts
        ));
        if engine.event_cursor() != self.snapshot.source_cursor {
            ui.colored_label(Color32::YELLOW, "Session changed; cancel and restart this route.");
        }
        ui.horizontal(|ui| {
            if ui.button("Choose start pad").clicked() {
                self.picking = Picking::Start;
            }
            if ui.button("Choose target pad").clicked() {
                self.picking = Picking::Target;
            }
        });
        let mut finish = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            finish = ui.add_enabled(self.start.is_some() && self.target.is_some(), egui::Button::new("Finish pad route")).clicked();
            cancel = ui.button("Cancel pad route").clicked();
        });
        (finish, cancel)
    }
    pub(crate) fn finish(&self, engine: &EngineHandle) -> Result<f64, String> {
        let (Some(start), Some(target)) = self.endpoints() else { return Err("Choose start and target pads before finishing".into()) };
        let input = RoutePadPolyline {
            expected_project: self.snapshot.project_id,
            expected_cursor: self.snapshot.source_cursor,
            start,
            target,
            interior: self.interior.clone(),
            grid: self.grid,
            width: self.width,
            rules: self.rules.clone(),
        };
        let now = Instant::now();
        UiAction::RoutePadPolyline.run(engine, serde_json::to_value(input).expect("pad route serializes")).map_err(|e| e.to_string())?;
        Ok(now.elapsed().as_secs_f64() * 1000.0)
    }
    fn pick_pad(&self, position: Pos2, response: &Response, camera: &Camera, layers: &LayerStack) -> Option<(usize, f32)> {
        let p = camera.screen_to_world(position, response.rect);
        let query = Point::from_mm(p.x, p.y);
        let tolerance = Nm::from_mm(6.0 / camera.zoom);
        self.index
            .query_point(query, tolerance)
            .filter_map(|(i, _)| {
                let row = &self.snapshot.endpoints[i];
                if !visible(row, layers) {
                    return None;
                }
                let c = center(row);
                let distance = position.distance(camera.world_to_screen(c, response.rect));
                let corners = row.corners_mm.map(|p| camera.world_to_screen(WorldPos::new(p[0], p[1]), response.rect));
                (inside(position, corners) || distance <= 6.0).then_some((i, distance))
            })
            .min_by(|(a, ad), (b, bd)| ad.total_cmp(bd).then(self.snapshot.endpoints[*a].id.cmp(&self.snapshot.endpoints[*b].id)))
    }
    /// Returns cancel/finish intents; geometry and selection stay local.
    #[allow(clippy::too_many_arguments)] // Shared canvas context plus local error output.
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
        if !enabled || lost || engine.read(|p| p.id) != self.snapshot.project_id || ui.input(|i| i.key_pressed(Key::Escape)) {
            return (true, false);
        }
        let keyboard = !ui.ctx().egui_wants_keyboard_input();
        if keyboard && ui.input(|i| i.key_pressed(Key::Backspace)) {
            self.interior.pop();
            *error = None;
        }
        self.hover = response.hover_pos().and_then(|p| {
            let p = camera.screen_to_world(p, response.rect);
            if !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 10000.0 || p.y.abs() > 10000.0 {
                return None;
            }
            viamind_router::snap(Point::from_mm(p.x, p.y), self.grid).ok()
        });
        self.hovered_pad = response.hover_pos().and_then(|p| self.pick_pad(p, response, camera, layers)).map(|p| p.0);
        if !ui.input(|i| i.key_down(Key::Space))
            && response.clicked_by(PointerButton::Primary)
            && let Some(position) = response.hover_pos()
        {
            let p = camera.screen_to_world(position, response.rect);
            let query = Point::from_mm(p.x, p.y);
            let tolerance = Nm::from_mm(6.0 / camera.zoom);
            let pad = self.hovered_pad.map(|i| (i, 0.0));
            let result = (|| -> Result<(), String> {
                if self.picking == Picking::Interior {
                    if pad.is_some() {
                        return Err("Use Choose start pad or Choose target pad to change endpoints; click empty space for an interior vertex".into());
                    }
                    if let Some(point) = self.hover
                        && self.interior.last() != Some(&point)
                        && self.interior.len() < 1023
                    {
                        self.interior.push(point);
                    }
                    return Ok(());
                }
                if let Some((i, _)) = pad {
                    if let Some(e) = &self.snapshot.endpoints[i].unsupported_reason {
                        return Err(e.clone());
                    }
                    if self.picking == Picking::Start {
                        self.start = Some(i);
                        self.target = None;
                        self.interior.clear();
                        self.picking = Picking::Target;
                    } else if let Some(a) = self.start {
                        self.compatible(a, i)?;
                        self.target = Some(i);
                        self.picking = Picking::Interior;
                    } else {
                        return Err("Choose the start pad first".into());
                    }
                    return Ok(());
                }
                if self.picking == Picking::Start {
                    let guide = self
                        .guide_index
                        .query_point(query, tolerance)
                        .filter_map(|(i, _)| {
                            let pair = self.guides[i];
                            let a = &self.snapshot.endpoints[pair[0]];
                            let b = &self.snapshot.endpoints[pair[1]];
                            if !visible(a, layers) || !visible(b, layers) {
                                return None;
                            }
                            let distance = segment_distance(
                                position,
                                camera.world_to_screen(center(a), response.rect),
                                camera.world_to_screen(center(b), response.rect),
                            );
                            (distance <= 6.0).then_some((pair, distance))
                        })
                        .min_by(|(a, ad), (b, bd)| ad.total_cmp(bd).then(a.cmp(b)));
                    if let Some((pair, _)) = guide {
                        self.compatible(pair[0], pair[1])?;
                        if let Some(e) = &self.snapshot.endpoints[pair[0]].unsupported_reason {
                            return Err(e.clone());
                        }
                        self.start = Some(pair[0]);
                        self.target = Some(pair[1]);
                        self.interior.clear();
                        self.picking = Picking::Interior;
                        return Ok(());
                    }
                }
                Err("Choose a visible explicit pad or a compatible connection guide".into())
            })();
            *error = result.err();
        }
        let screen = |p: Point| camera.world_to_screen(world(p), response.rect);
        let mut points = Vec::with_capacity(self.interior.len() + 3);
        if let Some(i) = self.start
            && let Some(p) = self.snapshot.endpoints[i].center_nm
        {
            points.push(p);
        }
        points.extend_from_slice(&self.interior);
        if self.picking == Picking::Interior
            && let Some(p) = self.hover
        {
            points.push(p);
        }
        if let Some(i) = self.target
            && let Some(p) = self.snapshot.endpoints[i].center_nm
        {
            points.push(p);
        }
        let painter = ui.painter().with_clip_rect(response.rect);
        let stroke = Stroke::new(((self.width.to_mm() * camera.zoom) as f32).max(1.0), Color32::from_rgb(255, 174, 64));
        if let Some(i) = self.hovered_pad {
            let corners = self.snapshot.endpoints[i].corners_mm.map(|p| camera.world_to_screen(WorldPos::new(p[0], p[1]), response.rect));
            for j in 0..4 {
                painter.line_segment([corners[j], corners[(j + 1) % 4]], Stroke::new(2.0, Color32::YELLOW));
            }
        }
        for pair in points.windows(2) {
            painter.line_segment([screen(pair[0]), screen(pair[1])], stroke);
        }
        for point in points {
            painter.circle_stroke(screen(point), 5.0, Stroke::new(1.0, Color32::WHITE));
        }
        (false, keyboard && ui.input(|i| i.key_pressed(Key::Enter)))
    }
}
