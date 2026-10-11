//! Noninteractive PCB attention above copper, clipped to the canvas.
use egui::{Color32, FontId, Rect, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::{ViamindApp, camera::WorldPos};

const COLORS: [Color32; 8] = [
    Color32::from_rgb(110, 220, 255),
    Color32::from_rgb(255, 165, 100),
    Color32::from_rgb(190, 160, 255),
    Color32::from_rgb(100, 245, 180),
    Color32::from_rgb(255, 125, 190),
    Color32::from_rgb(245, 225, 100),
    Color32::from_rgb(140, 185, 255),
    Color32::from_rgb(200, 240, 130),
];

impl ViamindApp {
    pub(crate) fn paint_agent_presence(&self, ui: &Ui, canvas: Rect) {
        let Some(design) = &self.design else { return };
        let snapshot = design.engine.query_agent_presence();
        if snapshot.agents.is_empty() {
            return;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        let painter = ui.painter_at(canvas);
        for agent in snapshot.agents {
            let color = COLORS[usize::from(agent.palette_index) % COLORS.len()];
            let world = |p: viamind_geom::Point| WorldPos::new(p.x.to_mm(), p.y.to_mm());
            for id in agent.selection {
                if let Some(bounds) = self.selection.visible_bounds(id) {
                    let rect = Rect::from_two_pos(
                        self.canvas.camera.world_to_screen(world(bounds.min), canvas),
                        self.canvas.camera.world_to_screen(world(bounds.max), canvas),
                    );
                    if rect.intersects(canvas) {
                        painter.rect_stroke(rect.expand(4.0), 2.0, Stroke::new(2.0, color), StrokeKind::Outside);
                    }
                }
            }
            let at = self.canvas.camera.world_to_screen(WorldPos::new(agent.cursor.x.to_mm(), agent.cursor.y.to_mm()), canvas);
            if !canvas.contains(at) {
                continue;
            }
            painter.add(Shape::convex_polygon(
                vec![at, at + Vec2::new(0.0, 13.0), at + Vec2::new(4.0, 9.0), at + Vec2::new(10.0, 10.0)],
                color,
                Stroke::new(1.0, Color32::BLACK),
            ));
            let text = if agent.working_on.is_empty() {
                format!("{} #{}", agent.display_name, agent.connection)
            } else {
                format!("{} #{} · {}", agent.display_name, agent.connection, agent.working_on)
            };
            let galley = painter.layout(text, FontId::proportional(12.0), color, (canvas.width() - 16.0).clamp(1.0, 300.0));
            let min = egui::pos2(
                (at.x + 12.0).min(canvas.right() - galley.size().x - 4.0).max(canvas.left() + 4.0),
                (at.y + 3.0).min(canvas.bottom() - galley.size().y - 4.0).max(canvas.top() + 4.0),
            );
            let background = Rect::from_min_size(min, galley.size()).expand(3.0);
            painter.rect_filled(background, 3.0, Color32::from_black_alpha(220));
            painter.galley(min, galley, color);
        }
    }
}
