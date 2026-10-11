//! A synthetic board scene for exercising the canvas before the real document model exists.

use crate::camera::{WorldPos, WorldRect};

/// Which (placeholder) layer an item is drawn on; picks its colour and draw order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    BottomCopper,
    TopCopper,
    Via,
}

impl Layer {
    /// Bottom to top, the order layers are painted in.
    pub const ALL: [Self; 3] = [Self::BottomCopper, Self::TopCopper, Self::Via];

    pub fn name(self) -> &'static str {
        match self {
            Self::BottomCopper => "Bottom copper",
            Self::TopCopper => "Top copper",
            Self::Via => "Vias",
        }
    }
}

/// A drawable primitive in world millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prim {
    /// A rectangular pad.
    Rect(WorldRect),
    /// A round pad or via.
    Circle { center: WorldPos, radius: f64 },
    /// A track segment of the given width.
    Segment {
        a: WorldPos,
        b: WorldPos,
        width: f64,
    },
}

impl Prim {
    /// Axis-aligned bounds, including track width.
    pub fn bounds(&self) -> WorldRect {
        match *self {
            Self::Rect(r) => r,
            Self::Circle { center, radius } => WorldRect::new(
                WorldPos::new(center.x - radius, center.y - radius),
                WorldPos::new(center.x + radius, center.y + radius),
            ),
            Self::Segment { a, b, width } => WorldRect::from_points(a, b).expand(width * 0.5),
        }
    }
}

/// One object in the scene, with its bounds cached for culling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub prim: Prim,
    pub layer: Layer,
    pub bounds: WorldRect,
}

impl Item {
    pub fn new(prim: Prim, layer: Layer) -> Self {
        Self {
            prim,
            layer,
            bounds: prim.bounds(),
        }
    }
}

/// A board outline and the items on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub board: WorldRect,
    pub items: Vec<Item>,
}

impl Scene {
    /// A deterministic pseudo-random board with `n` items: about 40 % rectangular pads, 15 % vias
    /// and 45 % track segments on top and bottom copper. The board grows with `n` so density stays
    /// roughly constant (about 1 item per mm²; 100k items is a 316 × 221 mm board).
    pub fn synthetic(n: usize) -> Self {
        let width = (n as f64).sqrt().max(40.0);
        let height = (width * 0.7).round();
        let board = WorldRect::new(WorldPos::new(0.0, 0.0), WorldPos::new(width, height));
        let mut rng = SplitMix64(0x05EE_D0FB_0A2D);
        let mut items = Vec::with_capacity(n);
        let inset = 1.0;
        for _ in 0..n {
            let x = rng.range(inset, width - inset);
            let y = rng.range(inset, height - inset);
            let copper = if rng.next_f64() < 0.6 {
                Layer::TopCopper
            } else {
                Layer::BottomCopper
            };
            let kind = rng.next_f64();
            let item = if kind < 0.40 {
                let hw = rng.range(0.15, 1.0);
                let hh = rng.range(0.15, 1.0);
                Item::new(
                    Prim::Rect(WorldRect::new(
                        WorldPos::new(x - hw, y - hh),
                        WorldPos::new(x + hw, y + hh),
                    )),
                    copper,
                )
            } else if kind < 0.55 {
                Item::new(
                    Prim::Circle {
                        center: WorldPos::new(x, y),
                        radius: rng.range(0.2, 0.45),
                    },
                    Layer::Via,
                )
            } else {
                // Tracks run in one of eight directions, as routed copper usually does.
                let dir = (rng.next_u64() % 8) as f64 * std::f64::consts::FRAC_PI_4;
                let len = rng.range(0.5, 8.0);
                let bx = (x + len * dir.cos()).clamp(inset, width - inset);
                let by = (y + len * dir.sin()).clamp(inset, height - inset);
                Item::new(
                    Prim::Segment {
                        a: WorldPos::new(x, y),
                        b: WorldPos::new(bx, by),
                        width: rng.range(0.1, 0.5),
                    },
                    copper,
                )
            };
            items.push(item);
        }
        // Paint order: bottom copper, then top copper, then vias.
        items.sort_by_key(|item| Layer::ALL.iter().position(|l| *l == item.layer));
        Self { board, items }
    }

    /// Bounds of the board and everything on it, for zoom to fit.
    pub fn bounds(&self) -> WorldRect {
        self.items.iter().fold(self.board, |acc, item| {
            WorldRect::new(
                WorldPos::new(
                    acc.min.x.min(item.bounds.min.x),
                    acc.min.y.min(item.bounds.min.y),
                ),
                WorldPos::new(
                    acc.max.x.max(item.bounds.max.x),
                    acc.max.y.max(item.bounds.max.y),
                ),
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
    fn items_are_sorted_by_paint_order() {
        let scene = Scene::synthetic(1_000);
        let order: Vec<usize> = scene
            .items
            .iter()
            .map(|i| Layer::ALL.iter().position(|l| *l == i.layer).unwrap_or(0))
            .collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]));
    }
}
