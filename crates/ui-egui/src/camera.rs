//! 2D canvas camera: world millimetres to screen points and back.
//!
//! World coordinates are plain `f64` millimetres for now, with +x right and +y down (the same
//! orientation as the screen). They switch to `viamind_geom::Nm` once that type lands; only this
//! module and [`crate::scene`] need to change.

use egui::{Pos2, Rect, Vec2, pos2};

/// Smallest zoom, in screen points per millimetre (a 10 m wide view on a 1000 pt canvas).
pub const MIN_ZOOM: f64 = 0.1;
/// Largest zoom, in screen points per millimetre (1 µm is 100 points).
pub const MAX_ZOOM: f64 = 100_000.0;

/// A point in world space, in millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldPos {
    pub x: f64,
    pub y: f64,
}

impl WorldPos {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in world space, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRect {
    pub min: WorldPos,
    pub max: WorldPos,
}

impl WorldRect {
    pub const fn new(min: WorldPos, max: WorldPos) -> Self {
        Self { min, max }
    }

    /// The bounding box of two points, in any order.
    pub fn from_points(a: WorldPos, b: WorldPos) -> Self {
        Self {
            min: WorldPos::new(a.x.min(b.x), a.y.min(b.y)),
            max: WorldPos::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> WorldPos {
        WorldPos::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    /// Grows the rectangle by `margin` on every side.
    pub fn expand(&self, margin: f64) -> Self {
        Self {
            min: WorldPos::new(self.min.x - margin, self.min.y - margin),
            max: WorldPos::new(self.max.x + margin, self.max.y + margin),
        }
    }

    /// True when the two rectangles overlap or touch.
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }
}

/// Where the canvas looks and how far it is zoomed in.
///
/// `center` is the world point shown at the centre of the viewport and `zoom` is screen points
/// per millimetre. Every transform takes the viewport rectangle (the canvas area, in screen
/// points) so the camera does not need to track window resizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: WorldPos,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: WorldPos::default(),
            zoom: 10.0,
        }
    }
}

impl Camera {
    /// World millimetres to screen points.
    pub fn world_to_screen(&self, p: WorldPos, viewport: Rect) -> Pos2 {
        let c = viewport.center();
        pos2(
            c.x + ((p.x - self.center.x) * self.zoom) as f32,
            c.y + ((p.y - self.center.y) * self.zoom) as f32,
        )
    }

    /// Screen points to world millimetres.
    pub fn screen_to_world(&self, s: Pos2, viewport: Rect) -> WorldPos {
        let c = viewport.center();
        WorldPos::new(
            self.center.x + f64::from(s.x - c.x) / self.zoom,
            self.center.y + f64::from(s.y - c.y) / self.zoom,
        )
    }

    /// The part of the world visible in `viewport`.
    pub fn visible_world_rect(&self, viewport: Rect) -> WorldRect {
        WorldRect::from_points(
            self.screen_to_world(viewport.min, viewport),
            self.screen_to_world(viewport.max, viewport),
        )
    }

    /// Moves the view so the content follows a pointer drag of `delta` screen points.
    pub fn pan_by_screen(&mut self, delta: Vec2) {
        self.center.x -= f64::from(delta.x) / self.zoom;
        self.center.y -= f64::from(delta.y) / self.zoom;
    }

    /// Multiplies the zoom by `factor`, keeping the world point under `anchor` (a screen
    /// position) fixed on screen. The zoom is clamped to [`MIN_ZOOM`]..=[`MAX_ZOOM`].
    pub fn zoom_about(&mut self, anchor: Pos2, viewport: Rect, factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let fixed = self.screen_to_world(anchor, viewport);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let c = viewport.center();
        self.center.x = fixed.x - f64::from(anchor.x - c.x) / self.zoom;
        self.center.y = fixed.y - f64::from(anchor.y - c.y) / self.zoom;
    }

    /// Centres `bounds` in `viewport` at the largest zoom that shows all of it, leaving
    /// `margin_px` screen points free on every side.
    pub fn zoom_to_fit(&mut self, bounds: WorldRect, viewport: Rect, margin_px: f32) {
        let avail_w = f64::from((viewport.width() - 2.0 * margin_px).max(1.0));
        let avail_h = f64::from((viewport.height() - 2.0 * margin_px).max(1.0));
        let w = bounds.width().max(1e-6);
        let h = bounds.height().max(1e-6);
        self.zoom = (avail_w / w).min(avail_h / h).clamp(MIN_ZOOM, MAX_ZOOM);
        self.center = bounds.center();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(200.0, 30.0), egui::vec2(1000.0, 700.0))
    }

    fn assert_close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} != {b} (tolerance {tol})");
    }

    #[test]
    fn world_screen_world_round_trip() {
        let vp = viewport();
        for cam in [
            Camera::default(),
            Camera {
                center: WorldPos::new(123.4, -56.7),
                zoom: 0.37,
            },
            Camera {
                center: WorldPos::new(-1.0, 2.5),
                zoom: 4_000.0,
            },
        ] {
            for p in [
                WorldPos::new(0.0, 0.0),
                WorldPos::new(10.25, -3.5),
                WorldPos::new(cam.center.x + 0.1, cam.center.y - 0.2),
            ] {
                let back = cam.screen_to_world(cam.world_to_screen(p, vp), vp);
                // Screen positions are f32 points; allow a hundredth of a point of error.
                let tol = 0.01 / cam.zoom + 1e-9 * p.x.abs().max(p.y.abs());
                assert_close(back.x, p.x, tol);
                assert_close(back.y, p.y, tol);
            }
        }
    }

    #[test]
    fn screen_world_screen_round_trip() {
        let vp = viewport();
        let cam = Camera {
            center: WorldPos::new(50.0, 40.0),
            zoom: 12.5,
        };
        for s in [vp.min, vp.center(), vp.max, pos2(333.0, 444.0)] {
            let back = cam.world_to_screen(cam.screen_to_world(s, vp), vp);
            assert!((back - s).length() < 1e-3, "{s:?} -> {back:?}");
        }
    }

    #[test]
    fn viewport_center_shows_camera_center() {
        let vp = viewport();
        let cam = Camera {
            center: WorldPos::new(7.0, 8.0),
            zoom: 3.0,
        };
        assert_eq!(cam.world_to_screen(cam.center, vp), vp.center());
        assert_eq!(cam.screen_to_world(vp.center(), vp), cam.center);
    }

    #[test]
    fn zoom_about_point_keeps_point_fixed() {
        let vp = viewport();
        let mut cam = Camera {
            center: WorldPos::new(20.0, 10.0),
            zoom: 5.0,
        };
        let anchor = pos2(321.0, 654.0);
        for factor in [1.1, 2.0, 0.5, 0.9, 37.0, 1.0 / 37.0] {
            let before = cam.screen_to_world(anchor, vp);
            let zoom_before = cam.zoom;
            cam.zoom_about(anchor, vp, factor);
            assert_close(cam.zoom, zoom_before * factor, 1e-9);
            let after = cam.screen_to_world(anchor, vp);
            assert_close(after.x, before.x, 1e-9);
            assert_close(after.y, before.y, 1e-9);
            let screen = cam.world_to_screen(before, vp);
            assert!((screen - anchor).length() < 1e-3);
        }
    }

    #[test]
    fn zoom_is_clamped_and_still_keeps_point_fixed() {
        let vp = viewport();
        let mut cam = Camera::default();
        let anchor = pos2(900.0, 100.0);
        let before = cam.screen_to_world(anchor, vp);
        cam.zoom_about(anchor, vp, 1e12);
        assert_eq!(cam.zoom, MAX_ZOOM);
        let after = cam.screen_to_world(anchor, vp);
        assert_close(after.x, before.x, 1e-9);
        assert_close(after.y, before.y, 1e-9);
        cam.zoom_about(anchor, vp, 1e-12);
        assert_eq!(cam.zoom, MIN_ZOOM);
        cam.zoom_about(anchor, vp, f64::NAN);
        assert_eq!(cam.zoom, MIN_ZOOM);
    }

    #[test]
    fn pan_moves_content_with_the_pointer() {
        let vp = viewport();
        let mut cam = Camera::default();
        let p = WorldPos::new(3.0, 4.0);
        let before = cam.world_to_screen(p, vp);
        cam.pan_by_screen(egui::vec2(15.0, -40.0));
        let after = cam.world_to_screen(p, vp);
        assert!((after - before - egui::vec2(15.0, -40.0)).length() < 1e-3);
    }

    #[test]
    fn zoom_to_fit_shows_whole_bounds() {
        let vp = viewport();
        let mut cam = Camera::default();
        let bounds = WorldRect::new(WorldPos::new(-10.0, 5.0), WorldPos::new(90.0, 25.0));
        cam.zoom_to_fit(bounds, vp, 20.0);
        let visible = cam.visible_world_rect(vp);
        assert!(visible.min.x <= bounds.min.x && visible.max.x >= bounds.max.x);
        assert!(visible.min.y <= bounds.min.y && visible.max.y >= bounds.max.y);
        // Width is the limiting axis here: 100 mm across 960 points.
        assert_close(cam.zoom, 9.6, 1e-9);
        assert_eq!(cam.center, bounds.center());
    }
}
