//! The board canvas: input handling (pan, zoom, zoom to fit) and painting with egui shapes.

use egui::{Color32, CornerRadius, Key, PointerButton, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2};

use crate::camera::{Camera, WorldPos, WorldRect};
use crate::layers::LayerStack;
use crate::mesh_cache::MeshCache;
use crate::scene::{Item, Prim, Scene};

/// Screen points between the scene bounds and the canvas edge after zoom to fit.
const FIT_MARGIN: f32 = 24.0;
/// Zoom factor per point of scroll (one wheel notch is about 50 points in egui).
const SCROLL_ZOOM_SPEED: f64 = 0.0025;
/// Minor grid lines are at least this many screen points apart.
const MIN_GRID_SPACING: f64 = 12.0;

const CANVAS_BG: Color32 = Color32::from_rgb(14, 16, 20);
const BOARD_FILL: Color32 = Color32::from_rgb(28, 36, 32);

/// Canvas state that lives across frames.
#[derive(Debug)]
pub struct Canvas {
    pub camera: Camera,
    /// Zoom to fit on the next frame (set at start-up and when the scene changes).
    pub fit_pending: bool,
    /// World position under the pointer, if it is over the canvas.
    pub cursor_world: Option<WorldPos>,
    /// Items visible last frame (after culling).
    pub drawn: usize,
    /// The canvas area last frame, in screen points.
    pub viewport: Rect,
    /// Paint through the tessellation cache (default). `false` builds every shape each frame,
    /// the reference the cache is tested against.
    pub cached: bool,
    /// Tessellated scene tiles.
    pub cache: MeshCache,
    /// Reused across frames so uncached painting of 100k items does not reallocate.
    shapes: Vec<Shape>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            camera: Camera::default(),
            fit_pending: true,
            cursor_world: None,
            drawn: 0,
            viewport: Rect::NOTHING,
            cached: true,
            cache: MeshCache::default(),
            shapes: Vec::new(),
        }
    }
}

impl Canvas {
    /// Tells the canvas that the scene's items changed, so cached meshes are rebuilt.
    pub fn scene_changed(&mut self) {
        self.cache.invalidate();
    }

    /// Fills the remaining space of `ui` with the canvas, drawing `scene` with `layers`.
    pub fn show(&mut self, ui: &mut Ui, scene: &Scene, layers: &LayerStack) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let viewport = response.rect;
        self.viewport = viewport;

        // Zoom to fit: on request, at start-up, and on the Home key.
        if self.fit_pending || ui.input(|i| i.key_pressed(Key::Home)) {
            self.camera.zoom_to_fit(scene.bounds(), viewport, FIT_MARGIN);
            self.fit_pending = false;
        }

        // Pan: middle-drag, or space + primary drag.
        let space_held = ui.input(|i| i.key_down(Key::Space));
        if response.dragged_by(PointerButton::Middle) || (space_held && response.dragged_by(PointerButton::Primary)) {
            self.camera.pan_by_screen(response.drag_delta());
        }

        // Zoom about the pointer: scroll wheel, plus ctrl+scroll / pinch (egui's zoom delta).
        if let Some(pointer) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            let factor = f64::from(pinch) * (f64::from(scroll.y) * SCROLL_ZOOM_SPEED).exp();
            if factor != 1.0 {
                self.camera.zoom_about(pointer, viewport, factor);
            }
        }

        self.cursor_world = response.hover_pos().map(|p| self.camera.screen_to_world(p, viewport));

        if space_held && response.hovered() {
            ui.set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }

        self.paint(&painter, viewport, scene, layers);
    }

    fn paint(&mut self, painter: &egui::Painter, viewport: Rect, scene: &Scene, layers: &LayerStack) {
        let cam = self.camera;
        let to_screen = |p: WorldPos| cam.world_to_screen(p, viewport);
        let screen_rect = |r: WorldRect| Rect::from_min_max(to_screen(r.min), to_screen(r.max));

        painter.rect_filled(viewport, CornerRadius::ZERO, CANVAS_BG);
        let board = screen_rect(scene.board);
        painter.rect_filled(board, CornerRadius::ZERO, BOARD_FILL);
        paint_grid(painter, &cam, viewport);

        if self.cached {
            self.drawn = self.cache.paint(painter, scene, layers, &cam, viewport);
            return;
        }

        // Layer by layer in draw order, culled to the visible area (slightly grown so strokes at
        // the edge are not clipped).
        let visible = cam.visible_world_rect(viewport).expand(2.0 / cam.zoom);
        let mut shapes = std::mem::take(&mut self.shapes);
        shapes.clear();
        for layer in layers.draw_order().filter(|l| l.visible) {
            let color = layer.paint_color();
            let on_screen = scene.items.iter().filter(|i| i.layer == layer.id && i.bounds.intersects(&visible));
            shapes.extend(on_screen.map(|item| item_shape(item, color, &cam, viewport)));
        }
        self.drawn = shapes.len();
        painter.extend(shapes.drain(..));
        self.shapes = shapes;
    }
}

/// The egui shape for `item`, filled with `color`, seen through `cam` in `viewport`.
pub(crate) fn item_shape(item: &Item, color: Color32, cam: &Camera, viewport: Rect) -> Shape {
    let to_screen = |p: WorldPos| cam.world_to_screen(p, viewport);
    let zoom = cam.zoom as f32;
    match item.prim {
        Prim::Rect(r) => Shape::rect_filled(Rect::from_min_max(to_screen(r.min), to_screen(r.max)), CornerRadius::ZERO, color),
        Prim::Circle { center, radius } => Shape::circle_filled(to_screen(center), radius as f32 * zoom, color),
        Prim::Segment { a, b, width } => Shape::line_segment([to_screen(a), to_screen(b)], Stroke::new(width as f32 * zoom, color)),
    }
}

/// Grid spacing in millimetres: the smallest 1-2-5 step at least `min_px` points apart on screen.
pub fn grid_step(zoom: f64, min_px: f64) -> f64 {
    let raw = min_px / zoom;
    let decade = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * decade).find(|step| *step >= raw * (1.0 - 1e-9)).unwrap_or(10.0 * decade)
}

/// Draws minor grid lines at [`grid_step`] and brighter major lines every power of ten above it.
fn paint_grid(painter: &egui::Painter, cam: &Camera, viewport: Rect) {
    let step = grid_step(cam.zoom, MIN_GRID_SPACING);
    let major = 10f64.powf(step.log10().floor() + 1.0);
    // Fade minor lines in as they spread apart, so changing step does not pop.
    let spacing_px = step * cam.zoom;
    let fade = ((spacing_px - MIN_GRID_SPACING) / (2.0 * MIN_GRID_SPACING)).clamp(0.0, 1.0);
    let minor = Color32::from_white_alpha((6.0 + 10.0 * fade) as u8);
    let major_color = Color32::from_white_alpha(30);

    let visible = cam.visible_world_rect(viewport);
    let is_major = |v: f64| ((v / major).round() * major - v).abs() < step * 1e-3;

    let mut x = (visible.min.x / step).floor() * step;
    while x <= visible.max.x {
        let sx = cam.world_to_screen(WorldPos::new(x, 0.0), viewport).x;
        let color = if is_major(x) { major_color } else { minor };
        painter.line_segment([pos2(sx, viewport.top()), pos2(sx, viewport.bottom())], Stroke::new(1.0, color));
        x += step;
    }
    let mut y = (visible.min.y / step).floor() * step;
    while y <= visible.max.y {
        let sy = cam.world_to_screen(WorldPos::new(0.0, y), viewport).y;
        let color = if is_major(y) { major_color } else { minor };
        painter.line_segment([pos2(viewport.left(), sy), pos2(viewport.right(), sy)], Stroke::new(1.0, color));
        y += step;
    }

    // World origin crosshair.
    let o = cam.world_to_screen(WorldPos::default(), viewport);
    if viewport.expand(8.0).contains(o) {
        let s = Stroke::new(1.0, Color32::from_white_alpha(90));
        painter.line_segment([o - Vec2::X * 8.0, o + Vec2::X * 8.0], s);
        painter.line_segment([o - Vec2::Y * 8.0, o + Vec2::Y * 8.0], s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_step_follows_1_2_5() {
        assert_eq!(grid_step(12.0, 12.0), 1.0);
        assert_eq!(grid_step(10.0, 12.0), 2.0);
        assert_eq!(grid_step(4.0, 12.0), 5.0);
        assert_eq!(grid_step(1.0, 12.0), 20.0);
        assert!((grid_step(1000.0, 12.0) - 0.02).abs() < 1e-12);
    }

    #[test]
    fn grid_lines_stay_bounded_at_any_zoom() {
        for zoom in [crate::camera::MIN_ZOOM, 0.3, 1.0, 7.0, 99.0, 1e3, 1e5] {
            let step = grid_step(zoom, MIN_GRID_SPACING);
            assert!(step * zoom >= MIN_GRID_SPACING * 0.999, "zoom {zoom}");
            assert!(step * zoom < MIN_GRID_SPACING * 2.6, "zoom {zoom}");
        }
    }
}
