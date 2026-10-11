//! Local drag ghosts and shared guarded grouped part placement actions.
use crate::{actions::UiAction, board_scene, camera::Camera, canvas::item_shape};
use egui::{Color32, Key, PointerButton, Response, Ui};
use viamind_doc::{ObjectId, Part};
use viamind_engine::{EngineHandle, PartPlacement, TransformParts};
use viamind_geom::{Nm, Point};

#[derive(Debug)]
struct MoveGesture {
    project: ObjectId,
    parts: Vec<Part>,
    ids: Vec<ObjectId>,
    start: Point,
    delta: Point,
}
/// UI-only state; no document changes occur until pointer release.
#[derive(Debug, Default)]
pub struct PlacementGesture {
    moving: Option<MoveGesture>,
    suppress_release: bool,
    pub error: Option<String>,
}
fn at(camera: &Camera, response: &Response, p: egui::Pos2) -> Point {
    let world = camera.screen_to_world(p, response.rect);
    Point::from_mm(world.x, world.y)
}
impl PlacementGesture {
    pub fn moving(&self) -> bool {
        self.moving.is_some()
    }
    pub fn cancel(&mut self) {
        if self.moving.take().is_some() {
            self.suppress_release = true;
        }
    }
    /// Snapshot the whole selection once; engine validation guards every target at commit.
    pub fn transform_selected(&mut self, engine: &EngineHandle, operation: PartPlacement) {
        if self.moving.is_some() {
            return;
        }
        let ids = engine.selection().ids;
        let input = engine.read(|project| {
            ids.iter().map(|id| project.board.part(id).cloned()).collect::<Option<Vec<_>>>().map(|expected| TransformParts {
                expected_project: project.id,
                expected,
                operation,
            })
        });
        if let Some(input) = input {
            self.run(engine, input);
        } else {
            self.error = Some("Selection changed; select the parts again".into());
        }
    }
    fn run(&mut self, engine: &EngineHandle, input: TransformParts) {
        self.error =
            UiAction::TransformParts.run(engine, serde_json::to_value(input).expect("placement input serializes")).err().map(|e| e.to_string());
    }
    /// Returns true when this frame's pointer/escape handling belongs to moving rather than selection.
    pub fn show(&mut self, ui: &Ui, response: &Response, camera: &Camera, engine: &EngineHandle, drag_hit: bool, enabled: bool) -> bool {
        let mut consumed = self.moving.is_some() || self.suppress_release;
        let space = ui.input(|i| i.key_down(Key::Space));
        let keyboard = (response.hovered() || self.moving.is_some()) && !ui.ctx().egui_wants_keyboard_input();
        let lost_focus = ui.input(|i| !i.focused || i.events.iter().any(|event| matches!(event, egui::Event::WindowFocused(false))));
        if !enabled || lost_focus || space || (keyboard && ui.input(|i| i.key_pressed(Key::Escape))) {
            self.cancel();
        }
        if self.suppress_release {
            if !ui.input(|i| i.pointer.primary_down()) {
                self.suppress_release = false;
            }
            self.paint(ui, response, camera);
            return true;
        }
        if enabled
            && !space
            && response.drag_started_by(PointerButton::Primary)
            && drag_hit
            && ui.input(|i| i.modifiers.is_none())
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        {
            let ids = engine.selection().ids;
            self.moving = engine.read(|project| {
                ids.iter().map(|id| project.board.part(id).cloned()).collect::<Option<Vec<_>>>().map(|parts| MoveGesture {
                    project: project.id,
                    parts,
                    ids: ids.clone(),
                    start: at(camera, response, origin),
                    delta: Point::ORIGIN,
                })
            });
            self.error = None;
            consumed = true;
        }
        if let Some(gesture) = &mut self.moving {
            if engine.selection().ids != gesture.ids {
                self.cancel();
                self.error = Some("Selection changed; drag cancelled".into());
                self.paint(ui, response, camera);
                return true;
            }
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                let current = at(camera, response, pointer);
                if let (Some(x), Some(y)) = (current.x.0.checked_sub(gesture.start.x.0), current.y.0.checked_sub(gesture.start.y.0)) {
                    gesture.delta = Point::new(Nm(x), Nm(y));
                } else {
                    self.cancel();
                    self.error = Some("Move exceeds coordinate range; drag cancelled".into());
                    self.paint(ui, response, camera);
                    return true;
                }
            }
        }
        if response.drag_stopped_by(PointerButton::Primary)
            && let Some(gesture) = self.moving.take()
        {
            self.run(
                engine,
                TransformParts {
                    expected_project: gesture.project,
                    expected: gesture.parts,
                    operation: PartPlacement::Translate { delta: gesture.delta },
                },
            );
            consumed = true;
        }
        if enabled && keyboard && self.moving.is_none() && !consumed && !engine.selection().ids.is_empty() {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::R)) {
                self.transform_selected(engine, PartPlacement::Rotate { degrees: 90 });
            } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F)) {
                self.transform_selected(engine, PartPlacement::Flip);
            }
        }
        self.paint(ui, response, camera);
        consumed
    }
    fn paint(&self, ui: &Ui, response: &Response, camera: &Camera) {
        let painter = ui.painter_at(response.rect);
        if let Some(gesture) = &self.moving {
            let screen_delta = egui::vec2((gesture.delta.x.to_mm() * camera.zoom) as f32, (gesture.delta.y.to_mm() * camera.zoom) as f32);
            for part in &gesture.parts {
                if part.position.x.0.checked_add(gesture.delta.x.0).is_none() || part.position.y.0.checked_add(gesture.delta.y.0).is_none() {
                    continue;
                }
                for item in board_scene::part_items(part) {
                    let mut shape = item_shape(&item, Color32::from_rgba_unmultiplied(255, 200, 80, 150), camera, response.rect);
                    shape.translate(screen_delta);
                    painter.add(shape);
                }
            }
        }
        if let Some(error) = &self.error {
            painter.text(
                response.rect.left_top() + egui::vec2(8.0, 8.0),
                egui::Align2::LEFT_TOP,
                error,
                egui::FontId::proportional(13.0),
                Color32::LIGHT_RED,
            );
        }
    }
}
