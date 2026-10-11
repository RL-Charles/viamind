//! Pointer selection of board parts, with an R-tree broad phase over visible part bounds.

use egui::{Color32, Key, Modifiers, PointerButton, Pos2, Response, Shape, Stroke, StrokeKind, Ui};
use viamind_doc::{ObjectId, Project};
use viamind_engine::{EngineHandle, Origin, SelectionMode, SetSelection};
use viamind_geom::{Nm, Point, Polygon, Rect, SegmentIntersection, SpatialIndex};

use crate::{
    board_scene,
    camera::{Camera, WorldPos},
    layers::{LayerId, LayerStack},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionTool {
    #[default]
    Box,
    Lasso,
}

#[derive(Debug)]
struct Gesture {
    points: Vec<Point>,
    tool: SelectionTool,
    mode: SelectionMode,
}

/// UI gesture state; selected IDs themselves live in the engine.
#[derive(Debug, Default)]
pub struct Selection {
    pub tool: SelectionTool,
    index: SpatialIndex<ObjectId>,
    visible_layers: Vec<LayerId>,
    dirty: bool,
    gesture: Option<Gesture>,
    cancelled: bool,
}

fn point(p: WorldPos) -> Point {
    Point::from_mm(p.x, p.y)
}

fn world(p: Point) -> WorldPos {
    WorldPos::new(p.x.to_mm(), p.y.to_mm())
}

fn mode(modifiers: Modifiers) -> SelectionMode {
    match (modifiers.command || modifiers.ctrl, modifiers.shift) {
        (true, true) => SelectionMode::Remove,
        (true, false) => SelectionMode::Toggle,
        (false, true) => SelectionMode::Add,
        (false, false) => SelectionMode::Replace,
    }
}

impl Selection {
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Rebuild only when parts or visibility change, keeping picks cheap between edits.
    pub fn sync(&mut self, engine: &EngineHandle, layers: &LayerStack) {
        let visible: Vec<_> = layers.draw_order().filter(|layer| layer.visible && layer.opacity > 0.0).map(|layer| layer.id).collect();
        if self.dirty || self.visible_layers != visible {
            self.index = engine.read(|project| Self::build_index(project, &visible));
            self.visible_layers = visible;
            self.dirty = false;
        }
    }

    fn build_index(project: &Project, visible: &[LayerId]) -> SpatialIndex<ObjectId> {
        SpatialIndex::bulk_load(project.board.parts.values().filter_map(|part| {
            let rect = board_scene::part_items(part)
                .into_iter()
                .filter(|item| visible.contains(&item.layer))
                .map(|item| Rect::new(point(item.bounds.min), point(item.bounds.max)))
                .reduce(|a, b| a.union(&b))?;
            Some((part.id, rect))
        }))
    }

    /// Closest visible bounds within the screen-sized picking tolerance. IDs break overlap ties.
    pub fn pick(&self, at: Point, tolerance: Nm) -> Option<ObjectId> {
        self.index
            .query_point(at, tolerance)
            .min_by(|(a_id, a), (b_id, b)| a.distance_to_point(at).total_cmp(&b.distance_to_point(at)).then(a_id.cmp(b_id)))
            .map(|(id, _)| id)
    }

    pub fn box_ids(&self, start: Point, end: Point) -> Vec<ObjectId> {
        let bounds = Rect::new(start, end);
        let enclosed = start.x <= end.x;
        self.index.query_rect(&bounds).filter(|(_, rect)| !enclosed || bounds.contains_rect(rect)).map(|(id, _)| id).collect()
    }

    /// Exact polygon/bounds intersection after querying the lasso bounding box.
    pub fn lasso_ids(&self, ring: &Polygon) -> Vec<ObjectId> {
        if ring.points.len() < 3 {
            return Vec::new();
        }
        let Some(bounds) = ring.bbox() else { return Vec::new() };
        self.index
            .query_rect(&bounds)
            .filter(|(_, rect)| {
                let rectangle = Polygon::from_rect(rect);
                rect.corners().iter().any(|&p| ring.contains(p))
                    || ring.points.iter().any(|&p| rect.contains(p))
                    || ring.edges().any(|a| rectangle.edges().any(|b| !matches!(a.intersection(&b), SegmentIntersection::None)))
            })
            .map(|(id, _)| id)
            .collect()
    }

    pub fn select_all(&self, engine: &EngineHandle) {
        self.apply(engine, self.index.iter().map(|(id, _)| id).collect(), SelectionMode::Replace);
    }

    pub fn clear(&self, engine: &EngineHandle) {
        self.apply(engine, Vec::new(), SelectionMode::Replace);
    }

    fn apply(&self, engine: &EngineHandle, ids: Vec<ObjectId>, mode: SelectionMode) {
        // An agent may have deleted a hit between picking and committing. The engine rejects
        // the entire stale request, leaving the current selection consistent.
        let _ = engine.set_selection(SetSelection { ids, mode }, Origin::User);
    }

    /// Handle the canvas response and paint selection above the cached scene.
    pub fn show(&mut self, ui: &Ui, response: &Response, camera: &Camera, engine: &EngineHandle) {
        let viewport = response.rect;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let to_point = |p: Pos2| point(camera.screen_to_world(p, viewport));
        let to_screen = |p: Point| camera.world_to_screen(world(p), viewport);
        let space = ui.input(|i| i.key_down(Key::Space));
        let keyboard = (response.hovered() || self.gesture.is_some()) && !ui.ctx().egui_wants_keyboard_input();
        if keyboard && ui.input(|i| i.key_pressed(Key::Escape)) {
            if self.gesture.take().is_some() {
                self.cancelled = true;
            } else {
                self.clear(engine);
            }
        }
        if keyboard && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::A))) {
            self.select_all(engine);
        }
        if space {
            self.gesture = None;
            if ui.input(|i| i.pointer.primary_down()) {
                self.cancelled = true;
            }
        }
        if !space
            && !self.cancelled
            && response.drag_started_by(PointerButton::Primary)
            && let Some(start) = ui.input(|i| i.pointer.press_origin())
        {
            self.gesture = Some(Gesture { points: vec![to_point(start)], tool: self.tool, mode: mode(ui.input(|i| i.modifiers)) });
        }
        if let Some(gesture) = &mut self.gesture
            && let Some(at) = pointer
        {
            let p = to_point(at);
            if gesture.tool == SelectionTool::Box {
                gesture.points.truncate(1);
                gesture.points.push(p);
            } else if gesture.points.last().is_none_or(|&last| to_screen(last).distance(at) >= 2.0) {
                gesture.points.push(p);
            }
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(gesture) = self.gesture.take() {
                let ids = match gesture.tool {
                    SelectionTool::Box => self.box_ids(gesture.points[0], *gesture.points.last().unwrap()),
                    SelectionTool::Lasso => self.lasso_ids(&Polygon::new(gesture.points)),
                };
                self.apply(engine, ids, gesture.mode);
            }
        } else if !space
            && !self.cancelled
            && response.clicked_by(PointerButton::Primary)
            && let Some(at) = pointer
        {
            let ids = self.pick(to_point(at), Nm::from_mm(5.0 / camera.zoom)).into_iter().collect();
            self.apply(engine, ids, mode(ui.input(|i| i.modifiers)));
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.cancelled = false;
        }

        let painter = ui.painter_at(viewport);
        let yellow = Color32::from_rgb(255, 214, 80);
        for id in engine.selection().ids {
            if let Some(rect) = self.index.get(id) {
                let screen = egui::Rect::from_two_pos(to_screen(rect.min), to_screen(rect.max)).expand(3.0);
                painter.rect_stroke(screen, 2.0, Stroke::new(2.0, yellow), StrokeKind::Outside);
            }
        }
        if let Some(gesture) = &self.gesture {
            let points: Vec<_> = gesture.points.iter().map(|&p| to_screen(p)).collect();
            match gesture.tool {
                SelectionTool::Box => {
                    let color = if gesture.points[0].x <= gesture.points.last().unwrap().x { yellow } else { Color32::LIGHT_BLUE };
                    let rect = egui::Rect::from_two_pos(points[0], *points.last().unwrap());
                    painter.rect_filled(rect, 0.0, color.gamma_multiply(0.12));
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
                }
                SelectionTool::Lasso => {
                    painter.add(Shape::closed_line(points, Stroke::new(1.5, yellow)));
                }
            }
        }
    }
}
