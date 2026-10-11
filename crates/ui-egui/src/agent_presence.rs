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
        for agent in snapshot.agents.into_iter().filter(|agent| matches!(agent.surface, viamind_engine::PresenceSurface::Pcb)) {
            let color = COLORS[usize::from(agent.palette_index) % COLORS.len()];
            let world = |p: viamind_geom::Point| WorldPos::new(p.x.to_mm(), p.y.to_mm());
            for id in &agent.selection {
                if let Some(bounds) = self.selection.visible_bounds(*id) {
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
            paint_cursor(&painter, canvas, at, &agent, color);
        }
    }
}

fn paint_cursor(painter: &egui::Painter, canvas: Rect, at: egui::Pos2, agent: &viamind_engine::AgentPresence, color: Color32) {
    if !canvas.contains(at) {
        return;
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

impl ViamindApp {
    pub(crate) fn paint_schematic_agent_presence(
        &self,
        ui: &Ui,
        canvas: Rect,
        sheet: viamind_doc::ObjectId,
        content: Option<&viamind_doc::schematic::SchematicContent>,
    ) {
        let Some(design) = &self.design else { return };
        let snapshot = design.engine.query_agent_presence();
        if snapshot.agents.is_empty() {
            return;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        let painter = ui.painter_at(canvas);
        let screen = |p: viamind_doc::schematic::Coordinate| self.schematic.camera.world_to_screen(WorldPos::new(p.x.to_mm(), p.y.to_mm()), canvas);
        for agent in snapshot.agents.into_iter().filter(|agent| agent.surface == viamind_engine::PresenceSurface::Schematic { sheet }) {
            let color = COLORS[usize::from(agent.palette_index) % COLORS.len()];
            if let Some(content) = content {
                for id in &agent.selection {
                    let bounds = if let Some(symbol) = content.symbols.get(id) {
                        let mut points = crate::schematic::body_corners(symbol).into_iter().map(&screen);
                        let first = points.next().expect("four body corners");
                        Some(points.fold(Rect::from_min_max(first, first), |rect, p| rect.union(Rect::from_min_max(p, p))))
                    } else if let Some(wire) = content.wires.get(id) {
                        Some(Rect::from_two_pos(screen(wire.start), screen(wire.end)))
                    } else {
                        None
                    };
                    if let Some(bounds) = bounds {
                        if bounds.expand(4.0).intersects(canvas) {
                            painter.rect_stroke(bounds.expand(4.0), 2.0, Stroke::new(2.0, color), StrokeKind::Outside);
                        }
                    } else if let Some(point) = content.junctions.get(id).map(|j| j.position).or_else(|| content.labels.get(id).map(|l| l.position)) {
                        let at = screen(point);
                        if canvas.contains(at) {
                            painter.circle_stroke(at, 7.0, Stroke::new(2.0, color));
                        }
                    }
                }
            }
            let at = self.schematic.camera.world_to_screen(WorldPos::new(agent.cursor.x.to_mm(), agent.cursor.y.to_mm()), canvas);
            paint_cursor(&painter, canvas, at, &agent, color);
        }
    }
}
