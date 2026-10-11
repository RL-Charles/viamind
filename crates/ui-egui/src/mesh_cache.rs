//! Cached tessellation of the scene, so large boards do not re-tessellate every frame.
//!
//! The scene is split into tiles: a coarse grid over the scene bounds, one tile per layer and
//! cell, each holding the items whose bounds centre falls in the cell. A tile is tessellated by
//! egui itself ([`egui::Context::tessellate`], so the same options and pre-rasterised discs apply)
//! the first time it is visible, from exactly the shapes the uncached path paints, and kept as an
//! [`egui::Mesh`] together with the camera it was built for.
//!
//! Later frames only map the cached vertex positions to the current camera (one multiply-add per
//! vertex) and hand the meshes to egui as [`Shape::Mesh`], which egui appends without
//! re-tessellating. A tile is rebuilt when the scene changes, when tessellation inputs change
//! (pixels per point, options, font atlas size), when its layer's colour or opacity changes, or
//! when the zoom has moved more than [`ZOOM_BUCKET`] from the zoom it was built at. Between
//! rebuilds the geometry is exact; only the one-pixel anti-aliasing fringe scales with the zoom
//! ratio (at most 9 %). Hiding, showing and reordering layers needs no rebuild.
//!
//! Tiles are emitted layer by layer in the [`LayerStack`] draw order, as uncached drawing does.
//! Within a layer every item has the same colour, and blending one colour over itself gives the
//! same result in either order, so the different item order inside a layer does not show.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use egui::epaint::{ClippedShape, Mesh, Primitive, TessellationOptions};
use egui::{Color32, Painter, Pos2, Rect, Shape};

use crate::camera::{Camera, WorldPos, WorldRect};
use crate::canvas::item_shape;
use crate::layers::{LayerId, LayerStack};
use crate::scene::Scene;

/// A cached tile is rebuilt when the zoom differs from its build zoom by more than this factor
/// (2^(1/8), about 9 %). Inside the bucket, panning and zooming just transform the cached mesh.
pub const ZOOM_BUCKET: f64 = 1.090_507_732_665_257_7;

/// Target number of items per grid cell (all layers together).
const ITEMS_PER_CELL: f64 = 512.0;
/// Smallest grid cell edge, in millimetres.
const MIN_CELL_MM: f64 = 2.0;

/// Inputs to egui's tessellator that change the output; a change drops every cached mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TessKey {
    pixels_per_point: f32,
    options: TessellationOptions,
    atlas_size: [usize; 2],
}

/// A tessellated tile and the camera it was built for.
struct Built {
    /// Zoom at build time.
    zoom: f64,
    /// Screen position of the world origin at build time (screen = origin + world * zoom).
    origin: [f64; 2],
    /// Vertex positions as tessellated, so transforms never accumulate rounding error.
    base: Vec<Pos2>,
    /// The mesh handed to egui; positions are `base * scale + offset`.
    mesh: Arc<Mesh>,
    /// The `(scale, offset x, offset y)` currently applied to `mesh`.
    applied: [f32; 3],
}

/// The items of one layer inside one grid cell.
struct Tile {
    /// Union of the items' bounds, for culling.
    bounds: WorldRect,
    /// Indices into [`Scene::items`], in scene order.
    items: Vec<u32>,
    built: Option<Built>,
}

/// The tiles of one layer.
struct LayerTiles {
    layer: LayerId,
    /// The fill colour the built meshes have; they are rebuilt when the layer's colour differs.
    color: Option<Color32>,
    tiles: Vec<Tile>,
}

/// Counters from the last [`MeshCache::paint`], for tests and the status bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Tiles in the cache.
    pub tiles: usize,
    /// Tiles painted last frame.
    pub tiles_drawn: usize,
    /// Tiles (re)tessellated last frame.
    pub tiles_built: usize,
    /// Vertices handed to egui last frame.
    pub vertices: usize,
}

/// Tessellated scene tiles, kept across frames.
#[derive(Default)]
pub struct MeshCache {
    /// Identity of the scene the tiles were made for: item buffer address and length.
    scene_key: Option<(usize, usize)>,
    /// Bumped by [`Self::invalidate`]; the tiles are rebuilt when it differs from `built_revision`.
    revision: u64,
    built_revision: u64,
    tess_key: Option<TessKey>,
    layers: Vec<LayerTiles>,
    /// Index into `layers` by layer id.
    index: HashMap<LayerId, usize>,
    pub stats: CacheStats,
}

impl std::fmt::Debug for MeshCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshCache").field("revision", &self.revision).field("stats", &self.stats).finish_non_exhaustive()
    }
}

impl MeshCache {
    /// Drops all cached meshes; call when the scene's items change.
    pub fn invalidate(&mut self) {
        self.revision += 1;
    }

    /// Paints the scene items on visible layers in `viewport`, in the stack's draw order, and
    /// returns how many items were painted.
    pub fn paint(&mut self, painter: &Painter, scene: &Scene, layers: &LayerStack, cam: &Camera, viewport: Rect) -> usize {
        let scene_key = (scene.items.as_ptr() as usize, scene.items.len());
        if self.scene_key != Some(scene_key) || self.built_revision != self.revision {
            self.layers = make_tiles(scene);
            self.index = self.layers.iter().enumerate().map(|(i, l)| (l.layer, i)).collect();
            self.scene_key = Some(scene_key);
            self.built_revision = self.revision;
        }

        let ctx = painter.ctx();
        let tess_key = TessKey {
            pixels_per_point: painter.pixels_per_point(),
            options: ctx.tessellation_options(|o| *o),
            atlas_size: ctx.fonts(|f: &egui::epaint::FontsView<'_>| -> [usize; 2] { f.font_image_size() }),
        };
        if self.tess_key != Some(tess_key) {
            self.layers.iter_mut().for_each(LayerTiles::clear);
            self.tess_key = Some(tess_key);
        }

        let visible = cam.visible_world_rect(viewport).expand(2.0 / cam.zoom);
        let stale = |b: &Built| {
            let ratio = cam.zoom / b.zoom;
            !(1.0 / ZOOM_BUCKET..=ZOOM_BUCKET).contains(&ratio)
        };
        // Current mapping: screen = origin + world * zoom.
        let c = viewport.center();
        let origin = [f64::from(c.x) - cam.center.x * cam.zoom, f64::from(c.y) - cam.center.y * cam.zoom];

        let mut drawn = 0;
        let mut stats = CacheStats { tiles: self.layers.iter().map(|l| l.tiles.len()).sum(), ..CacheStats::default() };
        for layer in layers.draw_order().filter(|l| l.visible) {
            let Some(group) = self.index.get(&layer.id).map(|&i| &mut self.layers[i]) else { continue };
            let color = layer.paint_color();
            if group.color != Some(color) {
                group.clear();
                group.color = Some(color);
            }
            for tile in group.tiles.iter_mut().filter(|t| t.bounds.intersects(&visible)) {
                if tile.built.as_ref().is_none_or(stale) {
                    tile.built = Some(build_tile(tile, color, scene, cam, viewport, ctx, tess_key.pixels_per_point));
                    stats.tiles_built += 1;
                }
                let Some(built) = tile.built.as_mut() else { continue };
                built.apply(cam.zoom, origin);
                stats.tiles_drawn += 1;
                stats.vertices += built.mesh.vertices.len();
                drawn += if contains(&visible, &tile.bounds) {
                    tile.items.len()
                } else {
                    tile.items.iter().filter(|&&i| scene.items[i as usize].bounds.intersects(&visible)).count()
                };
                painter.add(Shape::Mesh(Arc::clone(&built.mesh)));
            }
        }
        self.stats = stats;
        drawn
    }
}

impl LayerTiles {
    /// Drops the built meshes.
    fn clear(&mut self) {
        self.color = None;
        for tile in &mut self.tiles {
            tile.built = None;
        }
    }
}

impl Built {
    /// Moves the vertices to where the current camera puts them.
    fn apply(&mut self, zoom: f64, origin: [f64; 2]) {
        // screen = origin + world * zoom and base = self.origin + world * self.zoom, so
        // screen = base * scale + (origin - self.origin * scale).
        let scale = zoom / self.zoom;
        let target = [scale as f32, (origin[0] - self.origin[0] * scale) as f32, (origin[1] - self.origin[1] * scale) as f32];
        if target == self.applied {
            return;
        }
        let [s, tx, ty] = target;
        // egui has dropped last frame's shapes by now, so this does not copy the mesh.
        let mesh = Arc::make_mut(&mut self.mesh);
        for (v, b) in mesh.vertices.iter_mut().zip(&self.base) {
            v.pos = Pos2::new(b.x * s + tx, b.y * s + ty);
        }
        self.applied = target;
    }
}

/// True when `outer` contains all of `inner`.
fn contains(outer: &WorldRect, inner: &WorldRect) -> bool {
    outer.min.x <= inner.min.x && outer.min.y <= inner.min.y && inner.max.x <= outer.max.x && inner.max.y <= outer.max.y
}

/// Groups the scene items into tiles, per layer (in id order) and grid cell.
fn make_tiles(scene: &Scene) -> Vec<LayerTiles> {
    let bounds = scene.bounds();
    let n = scene.items.len().max(1) as f64;
    let cell = (bounds.width() * bounds.height() * ITEMS_PER_CELL / n).sqrt().max(MIN_CELL_MM);
    let cols = (bounds.width() / cell).ceil().max(1.0) as usize;
    let rows = (bounds.height() / cell).ceil().max(1.0) as usize;

    let mut cells: BTreeMap<LayerId, Vec<Vec<u32>>> = BTreeMap::new();
    for (i, item) in scene.items.iter().enumerate() {
        let c = item.bounds.center();
        let col = (((c.x - bounds.min.x) / cell) as usize).min(cols - 1);
        let row = (((c.y - bounds.min.y) / cell) as usize).min(rows - 1);
        cells.entry(item.layer).or_insert_with(|| vec![Vec::new(); cols * rows])[row * cols + col].push(i as u32);
    }
    cells
        .into_iter()
        .map(|(layer, cells)| {
            let tiles = cells
                .into_iter()
                .filter(|items| !items.is_empty())
                .map(|items| {
                    let first = scene.items[items[0] as usize].bounds;
                    let bounds = items.iter().fold(first, |acc, &i| union(&acc, &scene.items[i as usize].bounds));
                    Tile { bounds, items, built: None }
                })
                .collect();
            LayerTiles { layer, color: None, tiles }
        })
        .collect()
}

fn union(a: &WorldRect, b: &WorldRect) -> WorldRect {
    WorldRect::new(WorldPos::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)), WorldPos::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)))
}

/// Tessellates `tile`, filled with `color`, for the current camera.
fn build_tile(tile: &Tile, color: Color32, scene: &Scene, cam: &Camera, viewport: Rect, ctx: &egui::Context, pixels_per_point: f32) -> Built {
    // No clipping: items outside the viewport come into view when the camera pans.
    let shapes = tile
        .items
        .iter()
        .map(|&i| ClippedShape { clip_rect: Rect::EVERYTHING, shape: item_shape(&scene.items[i as usize], color, cam, viewport) })
        .collect();
    let mut mesh = Mesh::default();
    for primitive in ctx.tessellate(shapes, pixels_per_point) {
        if let Primitive::Mesh(m) = primitive.primitive {
            mesh.append(m);
        }
    }
    let c = viewport.center();
    let origin = [f64::from(c.x) - cam.center.x * cam.zoom, f64::from(c.y) - cam.center.y * cam.zoom];
    let base = mesh.vertices.iter().map(|v| v.pos).collect();
    Built { zoom: cam.zoom, origin, base, mesh: Arc::new(mesh), applied: [1.0, 0.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_every_item_once_per_layer() {
        let scene = Scene::synthetic(20_000);
        let layers = make_tiles(&scene);
        assert_eq!(layers.len(), LayerStack::pcb_default(4).len());
        assert!(layers.windows(2).all(|w| w[0].layer < w[1].layer));
        let mut seen = vec![false; scene.items.len()];
        for group in &layers {
            for tile in &group.tiles {
                for &i in &tile.items {
                    let item = &scene.items[i as usize];
                    assert_eq!(item.layer, group.layer, "one layer per tile");
                    assert!(contains(&tile.bounds, &item.bounds));
                    assert!(!seen[i as usize]);
                    seen[i as usize] = true;
                }
                assert!(tile.items.windows(2).all(|w| w[0] < w[1]), "scene order inside a tile");
            }
        }
        assert!(seen.iter().all(|s| *s));
        assert!(layers.iter().find(|l| l.layer == LayerId::F_CU).is_some_and(|l| l.tiles.len() > 1));
    }

    #[test]
    fn empty_scene_has_no_tiles() {
        assert!(make_tiles(&Scene::synthetic(0)).is_empty());
    }
}
