//! A synthetic board scene for exercising the canvas before the real document model exists.

use crate::camera::{WorldPos, WorldRect};
use crate::layers::LayerId;

/// A drawable primitive in world millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prim {
    /// A rectangular pad.
    Rect(WorldRect),
    /// True transformed rectangle corners, used for explicit pads.
    Quad([WorldPos; 4]),
    /// A round pad or via.
    Circle { center: WorldPos, radius: f64 },
    /// A track segment of the given width.
    Segment { a: WorldPos, b: WorldPos, width: f64 },
}

impl Prim {
    /// Axis-aligned bounds, including track width.
    pub fn bounds(&self) -> WorldRect {
        match *self {
            Self::Rect(r) => r,
            Self::Quad(points) => points.iter().skip(1).fold(WorldRect::from_points(points[0], points[0]), |r, p| {
                WorldRect::new(WorldPos::new(r.min.x.min(p.x), r.min.y.min(p.y)), WorldPos::new(r.max.x.max(p.x), r.max.y.max(p.y)))
            }),
            Self::Circle { center, radius } => {
                WorldRect::new(WorldPos::new(center.x - radius, center.y - radius), WorldPos::new(center.x + radius, center.y + radius))
            }
            Self::Segment { a, b, width } => WorldRect::from_points(a, b).expand(width * 0.5),
        }
    }
}

/// One object in the scene, with its bounds cached for culling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub prim: Prim,
    pub layer: LayerId,
    pub bounds: WorldRect,
}

impl Item {
    pub fn new(prim: Prim, layer: LayerId) -> Self {
        Self { prim, layer, bounds: prim.bounds() }
    }
}

/// A board outline and the items on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub board: WorldRect,
    /// Actual substrate triangles, including cutouts; None uses the synthetic rectangle.
    pub substrate: Option<Vec<[WorldPos; 3]>>,
    pub items: Vec<Item>,
}

impl Scene {
    /// A deterministic pseudo-random four-layer board with `n` items (the layers of
    /// [`crate::layers::LayerStack::pcb_default`]): the board outline on Edge.Cuts, then about 40 %
    /// rectangles (pads on F.Cu or B.Cu, half of them with a solder mask opening), 13 % vias,
    /// 35 % tracks on all four copper layers and 13 % silkscreen strokes. The board grows with `n`
    /// so density stays roughly constant (about 1 item per mm²; 100k items is a 316 × 221 mm board).
    pub fn synthetic(n: usize) -> Self {
        let width = (n as f64).sqrt().max(40.0);
        let height = (width * 0.7).round();
        let board = WorldRect::new(WorldPos::new(0.0, 0.0), WorldPos::new(width, height));
        let mut rng = SplitMix64(0x05EE_D0FB_0A2D);
        let mut items = Vec::with_capacity(n);
        let corners = [WorldPos::new(0.0, 0.0), WorldPos::new(width, 0.0), WorldPos::new(width, height), WorldPos::new(0.0, height)];
        for (i, a) in corners.iter().enumerate().take(n) {
            items.push(Item::new(Prim::Segment { a: *a, b: corners[(i + 1) % 4], width: 0.2 }, LayerId::EDGE_CUTS));
        }
        let inset = 1.0;
        while items.len() < n {
            let x = rng.range(inset, width - inset);
            let y = rng.range(inset, height - inset);
            let front = rng.next_f64() < 0.6;
            let kind = rng.next_f64();
            if kind < 0.30 {
                let hw = rng.range(0.15, 1.0);
                let hh = rng.range(0.15, 1.0);
                let pad = WorldRect::new(WorldPos::new(x - hw, y - hh), WorldPos::new(x + hw, y + hh));
                items.push(Item::new(Prim::Rect(pad), if front { LayerId::F_CU } else { LayerId::B_CU }));
                if rng.next_f64() < 0.5 && items.len() < n {
                    let mask = if front { LayerId::F_MASK } else { LayerId::B_MASK };
                    items.push(Item::new(Prim::Rect(pad.expand(0.05)), mask));
                }
            } else if kind < 0.45 {
                items.push(Item::new(Prim::Circle { center: WorldPos::new(x, y), radius: rng.range(0.2, 0.45) }, LayerId::VIAS));
            } else {
                // Tracks and silkscreen run in one of eight directions, as routed copper usually does.
                let silk = kind >= 0.85;
                let dir = (rng.next_u64() % 8) as f64 * std::f64::consts::FRAC_PI_4;
                let len = if silk { rng.range(0.5, 3.0) } else { rng.range(0.5, 8.0) };
                let bx = (x + len * dir.cos()).clamp(inset, width - inset);
                let by = (y + len * dir.sin()).clamp(inset, height - inset);
                let (layer, w) = if silk {
                    (if front { LayerId::F_SILKS } else { LayerId::B_SILKS }, 0.15)
                } else {
                    let copper = [LayerId::F_CU, LayerId::F_CU, LayerId::IN1_CU, LayerId::IN2_CU, LayerId::B_CU];
                    (copper[(rng.next_u64() % 5) as usize], rng.range(0.1, 0.5))
                };
                items.push(Item::new(Prim::Segment { a: WorldPos::new(x, y), b: WorldPos::new(bx, by), width: w }, layer));
            }
        }
        Self { board, substrate: None, items }
    }

    /// Bounds of the board and everything on it, for zoom to fit.
    pub fn bounds(&self) -> WorldRect {
        self.items.iter().fold(self.board, |acc, item| {
            WorldRect::new(
                WorldPos::new(acc.min.x.min(item.bounds.min.x), acc.min.y.min(item.bounds.min.y)),
                WorldPos::new(acc.max.x.max(item.bounds.max.x), acc.max.y.max(item.bounds.max.y)),
            )
        })
    }
}

/// Small deterministic generator (SplitMix64), so the scene needs no `rand` dependency.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_has_n_items_on_the_board() {
        for n in [0, 1, 1_000, 10_000] {
            let scene = Scene::synthetic(n);
            assert_eq!(scene.items.len(), n);
            let board = scene.board.expand(1.0);
            for item in &scene.items {
                assert!(board.intersects(&item.bounds), "{item:?} is off the board");
            }
        }
    }

    #[test]
    fn synthetic_is_deterministic_and_mixed() {
        let a = Scene::synthetic(2_000);
        assert_eq!(a, Scene::synthetic(2_000));
        let count = |f: fn(&Prim) -> bool| a.items.iter().filter(|i| f(&i.prim)).count();
        assert!(count(|p| matches!(p, Prim::Rect(_))) > 600);
        assert!(count(|p| matches!(p, Prim::Circle { .. })) > 200);
        assert!(count(|p| matches!(p, Prim::Segment { .. })) > 700);
    }

    #[test]
    fn synthetic_uses_every_default_layer() {
        let scene = Scene::synthetic(5_000);
        let stack = crate::layers::LayerStack::pcb_default(4);
        assert!(scene.items.iter().all(|i| stack.get(i.layer).is_some()));
        for layer in stack.layers() {
            let count = scene.items.iter().filter(|i| i.layer == layer.id).count();
            assert!(count >= 4, "{} has {count} items", layer.name);
        }
    }
}
