//! Frozen explicit-pad picking; the command resolves identities again at commit.
use crate::{
    actions::UiAction,
    camera::{Camera, WorldPos},
    layers::{LayerId, LayerStack},
};
use egui::{Color32, Key, PointerButton, Pos2, Response, Stroke, Ui};
use std::{collections::BTreeMap, time::Instant};
use viamind_doc::CopperLayer;
use viamind_engine::{EngineHandle, PadEndpointId, RouteEndpoint, RouteEndpointSnapshot, RoutePadPolyline, RoutePadPosture, RoutePadViaPolyline};
use viamind_geom::{Nm, Point, Rect, SpatialIndex};
use viamind_router::{CornerMode, PadPathPlan, Posture, plan_pad_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picking {
    Start,
    Target,
    Interior,
    Via,
    After,
}
#[derive(Debug)]
pub(crate) struct PadDraft {
    snapshot: RouteEndpointSnapshot,
    pub(crate) token: viamind_doc::ObjectId,
    pub(crate) walkaround: bool,
    pub(crate) checked: Option<viamind_engine::CheckedWalkaround>,
    pub(crate) plan_requested: bool,
    pub(crate) plan_pending: bool,
    index: SpatialIndex<usize>,
    guide_index: SpatialIndex<usize>,
    guides: Vec<[usize; 2]>,
    picking: Picking,
    start: Option<usize>,
    target: Option<usize>,
    interior: Vec<Point>,
    after: Vec<Point>,
    via: Option<Point>,
    via_sizes: Option<(Nm, Nm)>,
    posture: Option<(CornerMode, Posture)>,
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
            token: viamind_doc::ObjectId::new(),
            walkaround: false,
            checked: None,
            plan_requested: false,
            plan_pending: false,
            index,
            guide_index,
            guides,
            picking: Picking::Start,
            start: None,
            target: None,
            interior: Vec::new(),
            after: Vec::new(),
            via: None,
            via_sizes: None,
            posture: None,
            hover: None,
            hovered_pad: None,
            width,
            grid,
            rules,
        }
    }
    pub(crate) fn with_walkaround(mut self) -> Self {
        self.walkaround = true;
        self
    }
    pub(crate) fn walkaround_input(&self) -> Option<viamind_engine::RoutePadWalkaround> {
        let (Some(start), Some(target)) = self.endpoints() else { return None };
        self.walkaround.then(|| viamind_engine::RoutePadWalkaround {
            expected_project: self.snapshot.project_id,
            expected_cursor: self.snapshot.source_cursor,
            start,
            target,
            grid: self.grid,
            width: self.width,
            rules: self.rules.clone(),
        })
    }
    pub(crate) fn with_via(mut self, diameter: Nm, drill: Nm) -> Self {
        self.via_sizes = Some((diameter, drill));
        self
    }
    pub(crate) fn with_posture(mut self, mode: CornerMode) -> Self {
        self.posture = Some((mode, Posture::Primary));
        self
    }
    pub(crate) fn planned_path(&self) -> Result<Option<PadPathPlan>, String> {
        if self.walkaround {
            return Ok(self.checked.as_ref().map(|c| c.plan.path.clone()));
        }
        let Some((mode, posture)) = self.posture else { return Ok(None) };
        let (Some(start), Some(target)) = (self.start, self.target) else { return Ok(None) };
        let start = self.snapshot.endpoints[start].center_nm.ok_or("Unsupported start pad")?;
        let target = self.snapshot.endpoints[target].center_nm.ok_or("Unsupported target pad")?;
        plan_pad_path(start, target, &self.interior, self.grid, mode, posture).map(Some).map_err(str::to_owned)
    }
    pub(crate) fn via(&self) -> Option<Point> {
        self.via
    }
    pub(crate) fn after(&self) -> &[Point] {
        &self.after
    }
    fn clear_geometry(&mut self) {
        self.checked = None;
        self.interior.clear();
        self.after.clear();
        self.via = None;
    }
    fn next_stage(&self) -> Picking {
        if self.via_sizes.is_some() { Picking::Via } else { Picking::Interior }
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
        if a.net != b.net {
            return Err("Choose a pad with the exact same assigned net".into());
        }
        if self.via_sizes.is_some() && a.layer == b.layer {
            return Err("Choose a pad on the opposite outer copper side for one through via".into());
        }
        if self.via_sizes.is_none() && a.layer != b.layer {
            return Err("Choose a pad on the same copper side; use Route pads through via for opposite sides".into());
        }
        if self.via_sizes.is_none() && a.center_nm == b.center_nm {
            return Err("Endpoint centers coincide".into());
        }
        Ok(())
    }
    pub(crate) fn panel(&mut self, ui: &mut Ui, engine: &EngineHandle) -> (bool, bool) {
        ui.separator();
        ui.heading(if self.via_sizes.is_some() { "Through-via route preview" } else { "Pad connection preview" });
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
            Picking::Interior if self.via_sizes.is_some() => ui.label("Editing start leg: click empty space to insert before via."),
            Picking::Interior if self.walkaround => ui.label("Choose endpoints, request plan, then Enter to finish."),
            Picking::Interior => ui.label("Click: insert grid vertex before target · Backspace: remove · Enter: finish"),
            Picking::Via => ui.label("Place via: click empty space on the grid."),
            Picking::After => ui.label("Editing target leg: click empty space to insert before target."),
        };
        if self.walkaround {
            ui.label("Static cardinal walkaround · rectangular outer board and cutouts");
            ui.label("Request a checked plan after choosing both pads. No manual waypoints or shove.");
            if ui
                .add_enabled(
                    self.start.is_some() && self.target.is_some() && !self.plan_pending && engine.event_cursor() == self.snapshot.source_cursor,
                    egui::Button::new("Plan static walkaround"),
                )
                .clicked()
            {
                self.checked = None;
                self.plan_requested = true;
            }
            if self.plan_pending {
                ui.label("Planning and checking captured snapshot…");
            }
            if let Some(checked) = &self.checked {
                ui.label(format!(
                    "Checked snapshot · {} segments · {} obstacles · {} nodes",
                    checked.plan.path.points.len() - 1,
                    checked.plan.work.obstacle_boxes,
                    checked.plan.work.graph_nodes
                ));
            }
        }
        if let Some((mode, posture)) = &mut self.posture {
            ui.label(match mode {
                CornerMode::Cardinal => "Cardinal corners",
                CornerMode::Octilinear => "45-degree corners",
            });
            if ui.button(format!("Posture: {}", posture.label(*mode))).clicked() {
                posture.toggle();
            }
            ui.label("Endpoint stubs preserve exact off-grid pad centers and may have arbitrary angles.");
            match self.planned_path() {
                Ok(Some(plan)) => {
                    ui.label(format!("{} generated segments · {} exact endpoint stubs", plan.points.len() - 1, plan.endpoint_stub_segments));
                }
                Err(error) => {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                Ok(None) => {}
            }
        }
        if let Some((diameter, drill)) = self.via_sizes {
            for (label, index) in [("Start", self.start), ("Target", self.target)] {
                if let Some(i) = index
                    && let Some(p) = self.snapshot.endpoints[i].center_nm
                {
                    ui.monospace(format!("{label} X {} Y {} nm", p.x.0, p.y.0));
                }
            }
            ui.label(format!("Via copper {:.6} / drill {:.6} mm", diameter.to_mm(), drill.to_mm()));
            if let Some(p) = self.via {
                ui.monospace(format!("Via X {:.6} Y {:.6} mm", p.x.to_mm(), p.y.to_mm()));
            }
            ui.label("V: replace via · Backspace: remove active vertex · Enter: finish");
            if self.start.is_some() && self.target.is_some() {
                if ui.button("Place via").clicked() {
                    self.picking = Picking::Via;
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.via.is_some(), egui::Button::new("Edit start leg")).clicked() {
                        self.picking = Picking::Interior;
                    }
                    if ui.add_enabled(self.via.is_some(), egui::Button::new("Edit target leg")).clicked() {
                        self.picking = Picking::After;
                    }
                });
            }
        }
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
        ui.label(if self.walkaround {
            "Only a complete checked snapshot plan is drawn. Commit recomputes; conservative guides are not physical certification."
        } else {
            "Orange preview is unchecked. Guide counts remain conservative after routing."
        });
        ui.label(format!(
            "{} interior vertices · {} unsupported pads · {} legacy parts omitted",
            self.interior.len() + self.after.len(),
            self.snapshot.unsupported_endpoints,
            self.snapshot.omitted_legacy_parts
        ));
        if engine.event_cursor() != self.snapshot.source_cursor {
            ui.colored_label(Color32::YELLOW, "Session changed; cancel and restart this route.");
        }
        ui.horizontal(|ui| {
            if ui.button("Choose start pad").clicked() {
                if self.walkaround {
                    self.checked = None;
                }
                self.picking = Picking::Start;
            }
            if ui.button("Choose target pad").clicked() {
                if self.walkaround {
                    self.checked = None;
                }
                self.picking = Picking::Target;
            }
        });
        let mut finish = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            finish = ui
                .add_enabled(
                    self.start.is_some()
                        && self.target.is_some()
                        && (self.via_sizes.is_none() || self.via.is_some())
                        && (!self.walkaround
                            || (self.checked.is_some() && !self.plan_pending && engine.event_cursor() == self.snapshot.source_cursor)),
                    egui::Button::new(if self.via_sizes.is_some() { "Finish via route" } else { "Finish pad route" }),
                )
                .clicked();
            cancel = ui.button("Cancel pad route").clicked();
        });
        (finish, cancel)
    }
    pub(crate) fn finish(&self, engine: &EngineHandle) -> Result<f64, String> {
        let (Some(start), Some(target)) = self.endpoints() else { return Err("Choose start and target pads before finishing".into()) };
        let now = Instant::now();
        let result = if self.walkaround {
            if self.checked.is_none() || self.plan_pending {
                return Err("Request a complete checked walkaround plan before finishing".into());
            }
            let input = self.walkaround_input().ok_or("Choose start and target pads")?;
            UiAction::RoutePadWalkaround.run(engine, serde_json::to_value(input).expect("walkaround serializes"))
        } else if let Some((mode, posture)) = self.posture {
            let input = RoutePadPosture {
                expected_project: self.snapshot.project_id,
                expected_cursor: self.snapshot.source_cursor,
                start,
                target,
                waypoints: self.interior.clone(),
                grid: self.grid,
                width: self.width,
                mode,
                posture,
                rules: self.rules.clone(),
            };
            UiAction::RoutePadPosture.run(engine, serde_json::to_value(input).expect("posture route serializes"))
        } else if let Some((via_diameter, via_drill)) = self.via_sizes {
            let via = self.via.ok_or("Place the through via before finishing")?;
            let input = RoutePadViaPolyline {
                expected_project: self.snapshot.project_id,
                expected_cursor: self.snapshot.source_cursor,
                start,
                target,
                via,
                before: self.interior.clone(),
                after: self.after.clone(),
                grid: self.grid,
                width: self.width,
                via_diameter,
                via_drill,
                rules: self.rules.clone(),
            };
            UiAction::RoutePadViaPolyline.run(engine, serde_json::to_value(input).expect("via route serializes"))
        } else {
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
            UiAction::RoutePadPolyline.run(engine, serde_json::to_value(input).expect("pad route serializes"))
        };
        result.map_err(|e| e.to_string())?;
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
        if self.walkaround && engine.event_cursor() != self.snapshot.source_cursor {
            self.checked = None;
        }
        let keyboard = !ui.ctx().egui_wants_keyboard_input();
        if keyboard && ui.input(|i| i.key_pressed(Key::Backspace)) {
            match self.picking {
                Picking::After => {
                    self.after.pop();
                }
                Picking::Via => {
                    self.via = None;
                }
                _ => {
                    self.interior.pop();
                }
            }

            *error = None;
        }
        if self.via_sizes.is_some() && keyboard && ui.input(|i| i.key_pressed(Key::V)) && self.start.is_some() && self.target.is_some() {
            self.picking = Picking::Via;
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
                if self.picking == Picking::Via {
                    if pad.is_some() {
                        return Err("Choose empty space for the through via".into());
                    }
                    self.via = Some(self.hover.ok_or("Choose a valid grid position for the via")?);
                    self.picking = Picking::Interior;
                    return Ok(());
                }
                if matches!(self.picking, Picking::Interior | Picking::After) {
                    if self.walkaround {
                        return Err("Static walkaround does not accept manual vertices; request a checked plan or choose endpoints".into());
                    }
                    if pad.is_some() {
                        return Err("Use Choose start pad or Choose target pad to change endpoints; click empty space for an interior vertex".into());
                    }
                    let max = if self.posture.is_some() {
                        510
                    } else if self.via_sizes.is_some() {
                        1022
                    } else {
                        1023
                    };
                    if self.interior.len() + self.after.len() >= max {
                        return Err(format!("At most {max} interior vertices are supported"));
                    }
                    let active = if self.picking == Picking::After { &mut self.after } else { &mut self.interior };
                    if let Some(point) = self.hover
                        && active.last() != Some(&point)
                    {
                        active.push(point);
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
                        self.clear_geometry();
                        self.picking = Picking::Target;
                    } else if let Some(a) = self.start {
                        self.compatible(a, i)?;
                        self.target = Some(i);
                        self.checked = None;
                        if self.via_sizes.is_some() {
                            self.clear_geometry();
                        }
                        self.picking = self.next_stage();
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
                        self.clear_geometry();
                        self.picking = self.next_stage();
                        return Ok(());
                    }
                }
                Err("Choose a visible explicit pad or a compatible connection guide".into())
            })();
            *error = result.err();
        }
        let screen = |p: Point| camera.world_to_screen(world(p), response.rect);
        let mut points = Vec::with_capacity(self.interior.len() + self.after.len() + 4);
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
        let preview_via = if self.picking == Picking::Via { self.hover.or(self.via) } else { self.via };
        if self.via_sizes.is_some() {
            if let Some(p) = preview_via {
                points.push(p);
            }
            points.extend_from_slice(&self.after);
            if self.picking == Picking::After
                && let Some(p) = self.hover
            {
                points.push(p);
            }
        }
        if let Some(i) = self.target
            && let Some(p) = self.snapshot.endpoints[i].center_nm
        {
            points.push(p);
        }
        if let Some((mode, posture)) = self.posture
            && let (Some(start), Some(target)) = (self.start, self.target)
            && let (Some(start), Some(target)) = (self.snapshot.endpoints[start].center_nm, self.snapshot.endpoints[target].center_nm)
        {
            let mut waypoints = self.interior.clone();
            if self.picking == Picking::Interior
                && waypoints.len() < 510
                && let Some(hover) = self.hover
            {
                waypoints.push(hover);
            }
            points = plan_pad_path(start, target, &waypoints, self.grid, mode, posture).map_or_else(|_| vec![], |plan| plan.points);
        }
        if self.walkaround {
            points = self.checked.as_ref().map_or_else(Vec::new, |c| c.plan.path.points.clone());
        }
        let painter = ui.painter().with_clip_rect(response.rect);
        let stroke = Stroke::new(((self.width.to_mm() * camera.zoom) as f32).max(1.0), Color32::from_rgb(255, 174, 64));
        if let Some(i) = self.hovered_pad {
            let corners = self.snapshot.endpoints[i].corners_mm.map(|p| camera.world_to_screen(WorldPos::new(p[0], p[1]), response.rect));
            for j in 0..4 {
                painter.line_segment([corners[j], corners[(j + 1) % 4]], Stroke::new(2.0, Color32::YELLOW));
            }
        }
        if self.via_sizes.is_none() || preview_via.is_some() {
            for pair in points.windows(2) {
                painter.line_segment([screen(pair[0]), screen(pair[1])], stroke);
            }
        }
        if let Some((diameter, drill)) = self.via_sizes
            && let Some(p) = preview_via
        {
            painter.circle_stroke(
                screen(p),
                ((diameter.to_mm() * camera.zoom / 2.0) as f32).max(3.0),
                Stroke::new(2.0, Color32::from_rgb(255, 174, 64)),
            );
            painter.circle_stroke(screen(p), ((drill.to_mm() * camera.zoom / 2.0) as f32).max(1.0), Stroke::new(1.0, Color32::WHITE));
        }
        for point in points {
            painter.circle_stroke(screen(point), 5.0, Stroke::new(1.0, Color32::WHITE));
        }
        (false, keyboard && ui.input(|i| i.key_pressed(Key::Enter)))
    }
}
