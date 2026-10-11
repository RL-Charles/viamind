//! The tessellation cache draws the same triangles as painting every shape each frame.
//!
//! Triangles are compared as multisets, plus the order in which layer colours appear: inside a
//! layer all items share one colour, so their order does not change the blended result.

use std::collections::HashMap;

use egui::epaint::{Primitive, Vertex};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use viamind_ui_egui::ViamindApp;
use viamind_ui_egui::layers::{LayerId, LayerStack};

const SCREEN: Rect = Rect { min: Pos2::ZERO, max: Pos2 { x: 1280.0, y: 800.0 } };

struct Run {
    ctx: egui::Context,
    app: ViamindApp,
    time: f64,
}

impl Run {
    fn new(objects: usize, cached: bool) -> Self {
        let mut app = ViamindApp::with_scene_size(objects);
        app.canvas.cached = cached;
        let mut run = Self { ctx: egui::Context::default(), app, time: 0.0 };
        for _ in 0..3 {
            run.frame(vec![]);
        }
        run
    }

    /// Runs one frame and returns the triangles painted on the canvas.
    fn frame(&mut self, events: Vec<Event>) -> Vec<[Vertex; 3]> {
        self.time += 1.0 / 60.0;
        let input = RawInput { screen_rect: Some(SCREEN), time: Some(self.time), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.show(ui, None));
        out.textures_delta.clear();
        let mut tris = Vec::new();
        // Only the canvas: the status bar shows timings, which differ from run to run.
        let canvas = self.app.canvas.viewport;
        for prim in self.ctx.tessellate(out.shapes, out.pixels_per_point) {
            if prim.clip_rect != canvas {
                continue;
            }
            if let Primitive::Mesh(mesh) = prim.primitive {
                for t in mesh.indices.as_chunks::<3>().0 {
                    let tri = [0, 1, 2].map(|k| mesh.vertices[t[k] as usize]);
                    // The cache keeps whole tiles, so it also sends triangles that lie entirely
                    // outside the canvas; the GPU clips those away, so leave them out here.
                    if Rect::from_points(&tri.map(|v| v.pos)).intersects(canvas) {
                        tris.push(tri);
                    }
                }
            }
        }
        tris
    }
}

/// Grid cell of a triangle's centroid, for matching triangles up to `tol` position errors.
fn cell(t: &[Vertex; 3], tol: f32) -> (i32, i32) {
    let size = tol.max(0.25);
    let c = (t[0].pos.to_vec2() + t[1].pos.to_vec2() + t[2].pos.to_vec2()) / 3.0;
    ((c.x / size).floor() as i32, (c.y / size).floor() as i32)
}

/// Same vertices (in any order) with positions within `tol` points.
fn same_triangle(a: &[Vertex; 3], b: &[Vertex; 3], tol: f32) -> bool {
    let close = |va: &Vertex, vb: &Vertex| (va.pos - vb.pos).length() <= tol && va.color == vb.color && (va.uv - vb.uv).length() <= 1e-6;
    a.iter().all(|va| b.iter().any(|vb| close(va, vb))) && b.iter().all(|vb| a.iter().any(|va| close(va, vb)))
}

/// Asserts the two lists hold the same triangles, in any order, with positions within `tol` points.
///
/// With a tolerance, a triangle that moved by up to `tol` may have crossed the canvas edge, so
/// unmatched triangles within `2 * tol` of the edge are allowed.
fn assert_same(cached: &[[Vertex; 3]], uncached: &[[Vertex; 3]], tol: f32, canvas: Rect) {
    let mut pending: HashMap<(i32, i32), Vec<[Vertex; 3]>> = HashMap::new();
    for t in uncached {
        pending.entry(cell(t, tol)).or_default().push(*t);
    }
    let mut unmatched = Vec::new();
    for t in cached {
        let (cx, cy) = cell(t, tol);
        let found = (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (cx + dx, cy + dy))).find_map(|k| {
            let list = pending.get_mut(&k)?;
            let i = list.iter().position(|u| same_triangle(t, u, tol))?;
            Some(list.swap_remove(i))
        });
        if found.is_none() {
            unmatched.push(*t);
        }
    }
    let mut left: Vec<_> = pending.into_values().flatten().collect();
    if tol > 0.0 {
        let inner = canvas.shrink(2.0 * tol);
        let inside = |t: &[Vertex; 3]| Rect::from_points(&t.map(|v| v.pos)).intersects(inner);
        unmatched.retain(inside);
        left.retain(inside);
    }
    assert!(
        unmatched.is_empty() && left.is_empty(),
        "only cached: {:?}\nonly uncached: {:?}",
        &unmatched[..unmatched.len().min(5)],
        &left[..left.len().min(5)]
    );
}

/// Input events, one frame each, of a middle-button drag from `from` in `steps` moves of `step`.
fn middle_drag(from: Pos2, step: egui::Vec2, steps: usize) -> Vec<Vec<Event>> {
    let button = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Middle, pressed, modifiers: Modifiers::NONE };
    let mut frames = vec![vec![Event::PointerMoved(from), button(from, true)]];
    frames.extend((1..=steps).map(|i| vec![Event::PointerMoved(from + step * i as f32)]));
    frames.push(vec![button(from + step * steps as f32, false)]);
    frames
}

#[test]
fn cached_frame_matches_uncached_at_fit() {
    let mut cached = Run::new(5_000, true);
    let mut uncached = Run::new(5_000, false);
    let a = cached.frame(vec![]);
    let b = uncached.frame(vec![]);
    assert_eq!(cached.app.canvas.drawn, 5_000);
    assert_eq!(cached.app.canvas.drawn, uncached.app.canvas.drawn);
    assert!(cached.app.canvas.cache.stats.tiles_drawn > 1);
    // Same camera the tiles were built for: bit-identical triangles.
    assert_eq!(a.len(), b.len());
    assert_same(&a, &b, 0.0, cached.app.canvas.viewport);
}

#[test]
fn cached_frame_matches_uncached_after_pan() {
    let mut cached = Run::new(5_000, true);
    let mut uncached = Run::new(5_000, false);
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for events in middle_drag(pos2(700.0, 400.0), vec2(-19.0, 13.0), 4) {
        a = cached.frame(events.clone());
        b = uncached.frame(events);
    }
    assert_eq!(cached.app.canvas.camera, uncached.app.canvas.camera);
    assert_ne!(cached.app.canvas.camera.center, Run::new(5_000, true).app.canvas.camera.center, "the drag panned");
    assert!(cached.app.canvas.drawn < 5_000);
    assert_eq!(cached.app.canvas.drawn, uncached.app.canvas.drawn);
    // Panning reuses the meshes. The tessellator snaps rectangles and axis-aligned tracks to the
    // pixel grid; a whole-pixel pan keeps that, up to coordinates within float error of a snapping
    // boundary, which may land one pixel apart.
    assert_eq!(cached.app.canvas.cache.stats.tiles_built, 0);
    assert_same(&a, &b, 1.0, cached.app.canvas.viewport);
}

#[test]
fn zoom_rebuilds_only_outside_the_bucket() {
    let mut run = Run::new(5_000, true);
    let tiles = run.app.canvas.cache.stats.tiles;
    assert!(tiles > 1);
    let viewport = Rect::from_min_max(pos2(200.0, 30.0), pos2(1280.0, 770.0));

    run.app.canvas.camera.zoom_about(pos2(640.0, 400.0), viewport, 1.05);
    run.frame(vec![]);
    assert_eq!(run.app.canvas.cache.stats.tiles_built, 0, "inside the zoom bucket");

    run.app.canvas.camera.zoom_about(pos2(640.0, 400.0), viewport, 1.2);
    run.frame(vec![]);
    let stats = run.app.canvas.cache.stats;
    assert!(stats.tiles_built > 0 && stats.tiles_built == stats.tiles_drawn, "{stats:?}");

    // Changing the scene rebuilds the tiles.
    run.app.set_scene_size(1_000);
    run.frame(vec![]);
    run.frame(vec![]);
    assert_eq!(run.app.canvas.drawn, 1_000);
    assert_ne!(run.app.canvas.cache.stats.tiles, tiles);
}

/// The layers in the order their colours first appear in `tris` (painting order). Triangles
/// with no fully covered vertex (anti-aliasing fringes, thin lines) are skipped.
fn layer_order(tris: &[[Vertex; 3]], layers: &LayerStack) -> Vec<LayerId> {
    let colors: HashMap<Color32, LayerId> = layers.layers().iter().map(|l| (l.paint_color(), l.id)).collect();
    let mut order: Vec<LayerId> = Vec::new();
    for tri in tris {
        if let Some(id) = tri.iter().find_map(|v| colors.get(&v.color))
            && order.last() != Some(id)
        {
            order.push(*id);
        }
    }
    order
}

#[test]
fn layer_edits_match_uncached_and_rebuild_only_on_colour() {
    let mut cached = Run::new(5_000, true);
    let mut uncached = Run::new(5_000, false);

    // Hide, reorder and lift the active layer: no rebuild.
    for run in [&mut cached, &mut uncached] {
        run.app.layers.set_visible(LayerId::IN1_CU, false);
        run.app.layers.move_layer(LayerId::B_CU, true);
        run.app.layers.set_active(LayerId::B_MASK);
        run.app.layers.active_on_top = true;
    }
    let (a, b) = (cached.frame(vec![]), uncached.frame(vec![]));
    assert_eq!(cached.app.canvas.cache.stats.tiles_built, 0);
    assert!(cached.app.canvas.drawn < 5_000);
    assert_eq!(cached.app.canvas.drawn, uncached.app.canvas.drawn);
    assert_same(&a, &b, 0.0, cached.app.canvas.viewport);
    let expected: Vec<LayerId> = cached.app.layers.draw_order().filter(|l| l.visible).map(|l| l.id).collect();
    assert_eq!(layer_order(&a, &cached.app.layers), expected);
    assert_eq!(layer_order(&b, &uncached.app.layers), expected);
    assert_eq!(expected.first(), Some(&LayerId::B_SILKS));
    assert_eq!(expected.last(), Some(&LayerId::B_MASK));

    // Recolour one layer and fade another: only their tiles are rebuilt.
    for run in [&mut cached, &mut uncached] {
        run.app.layers.get_mut(LayerId::F_CU).unwrap().color = Color32::from_rgb(20, 200, 90);
        run.app.layers.get_mut(LayerId::VIAS).unwrap().opacity = 0.4;
    }
    let (a, b) = (cached.frame(vec![]), uncached.frame(vec![]));
    let stats = cached.app.canvas.cache.stats;
    assert!(stats.tiles_built > 0 && stats.tiles_built < stats.tiles_drawn, "{stats:?}");
    assert_same(&a, &b, 0.0, cached.app.canvas.viewport);
    assert_eq!(layer_order(&a, &cached.app.layers), expected);
}
