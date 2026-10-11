//! R-tree spatial index of bounding boxes, keyed by object.
//!
//! [`SpatialIndex`] answers "what is under the cursor", "what is near this
//! track" and "what overlaps this region" over the bounding boxes of board
//! objects. It is a broad phase: callers refine its candidates with exact shape
//! tests (see [`crate::Shape`]).
//!
//! Internally boxes are kept as `f64` (exact for integers below 2^53 nm, about
//! 9000 km), so squared distances never overflow.

use std::collections::HashMap;
use std::hash::Hash;

use rstar::{AABB, PointDistance, RTree, RTreeObject};

use crate::point::Point;
use crate::rect::Rect;
use crate::units::Nm;

/// One indexed box.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry<K> {
    key: K,
    rect: Rect,
}

fn to_f64(p: Point) -> [f64; 2] {
    [p.x.0 as f64, p.y.0 as f64]
}

fn envelope(r: &Rect) -> AABB<[f64; 2]> {
    AABB::from_corners(to_f64(r.min), to_f64(r.max))
}

impl<K> RTreeObject for Entry<K> {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        envelope(&self.rect)
    }
}

impl<K> PointDistance for Entry<K> {
    fn distance_2(&self, p: &[f64; 2]) -> f64 {
        self.envelope().distance_2(p)
    }
}

/// A spatial index of `(key, bounding box)` pairs, at most one box per key.
///
/// `K` is the caller's object id (any small `Copy + Eq + Hash` type). Queries
/// return `(key, rect)` pairs in no particular order. All queries treat boxes
/// as closed: touching counts as overlapping.
///
/// ```
/// use viamind_geom::{Nm, Point, Rect, SpatialIndex};
///
/// let mut index = SpatialIndex::new();
/// index.insert(1u32, Rect::new(Point::from_mm(0.0, 0.0), Point::from_mm(1.0, 1.0)));
/// index.insert(2u32, Rect::new(Point::from_mm(5.0, 5.0), Point::from_mm(6.0, 6.0)));
/// let hits: Vec<u32> = index.query_point(Point::from_mm(1.05, 0.5), Nm::from_mm(0.1)).map(|(k, _)| k).collect();
/// assert_eq!(hits, [1]);
/// assert_eq!(index.nearest(Point::from_mm(4.0, 4.0)).map(|(k, _)| k), Some(2));
/// ```
#[derive(Clone, Debug)]
pub struct SpatialIndex<K> {
    tree: RTree<Entry<K>>,
    rects: HashMap<K, Rect>,
}

impl<K: Copy + Eq + Hash> Default for SpatialIndex<K> {
    fn default() -> Self {
        SpatialIndex { tree: RTree::new(), rects: HashMap::new() }
    }
}

impl<K: Copy + Eq + Hash> SpatialIndex<K> {
    /// An empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds an index from many pairs at once; much faster than inserting one by one.
    ///
    /// If a key repeats, the last box wins.
    pub fn bulk_load(items: impl IntoIterator<Item = (K, Rect)>) -> Self {
        let rects: HashMap<K, Rect> = items.into_iter().collect();
        let entries = rects.iter().map(|(&key, &rect)| Entry { key, rect }).collect();
        SpatialIndex { tree: RTree::bulk_load(entries), rects }
    }

    /// Number of indexed keys.
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// The box stored for `key`.
    pub fn get(&self, key: K) -> Option<Rect> {
        self.rects.get(&key).copied()
    }

    /// Indexes `key` with `rect`, replacing (and returning) any previous box for that key.
    pub fn insert(&mut self, key: K, rect: Rect) -> Option<Rect> {
        let old = self.rects.insert(key, rect);
        if let Some(old) = old {
            self.tree.remove(&Entry { key, rect: old });
        }
        self.tree.insert(Entry { key, rect });
        old
    }

    /// Moves an already indexed `key` to `rect`. Returns the previous box, or
    /// `None` (and changes nothing) if the key is not indexed.
    pub fn update(&mut self, key: K, rect: Rect) -> Option<Rect> {
        if self.rects.contains_key(&key) { self.insert(key, rect) } else { None }
    }

    /// Removes `key`, returning its box.
    pub fn remove(&mut self, key: K) -> Option<Rect> {
        let rect = self.rects.remove(&key)?;
        self.tree.remove(&Entry { key, rect });
        Some(rect)
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Every indexed pair, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (K, Rect)> + '_ {
        self.rects.iter().map(|(&k, &r)| (k, r))
    }

    /// Boxes that overlap or touch `area`.
    pub fn query_rect(&self, area: &Rect) -> impl Iterator<Item = (K, Rect)> + '_ {
        self.tree.locate_in_envelope_intersecting(envelope(area)).map(|e| (e.key, e.rect))
    }

    /// Boxes within `tolerance` of `p` (Euclidean distance to the box; zero inside).
    ///
    /// This is the hit test for a cursor or probe: pass the pick radius as `tolerance`.
    pub fn query_point(&self, p: Point, tolerance: Nm) -> impl Iterator<Item = (K, Rect)> + '_ {
        let t = tolerance.0.max(0) as f64;
        self.tree.locate_within_distance(to_f64(p), t * t).map(|e| (e.key, e.rect))
    }

    /// The box nearest to `p` (any box containing `p` is at distance zero).
    pub fn nearest(&self, p: Point) -> Option<(K, Rect)> {
        self.tree.nearest_neighbor(to_f64(p)).map(|e| (e.key, e.rect))
    }

    /// Boxes in order of increasing distance from `p`.
    pub fn nearest_iter(&self, p: Point) -> impl Iterator<Item = (K, Rect)> + '_ {
        self.tree.nearest_neighbor_iter(to_f64(p)).map(|e| (e.key, e.rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
        Rect::new(Point::new(Nm(x0), Nm(y0)), Point::new(Nm(x1), Nm(y1)))
    }

    fn keys(it: impl Iterator<Item = (u32, Rect)>) -> Vec<u32> {
        let mut v: Vec<u32> = it.map(|(k, _)| k).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn insert_update_remove() {
        let mut idx = SpatialIndex::new();
        assert_eq!(idx.insert(1, r(0, 0, 10, 10)), None);
        assert_eq!(idx.insert(2, r(20, 0, 30, 10)), None);
        assert_eq!(keys(idx.query_rect(&r(10, 0, 20, 0))), [1, 2]);
        assert_eq!(idx.update(1, r(100, 100, 110, 110)), Some(r(0, 0, 10, 10)));
        assert_eq!(idx.update(9, r(0, 0, 1, 1)), None);
        assert_eq!(idx.len(), 2);
        assert_eq!(keys(idx.query_rect(&r(0, 0, 15, 15))), Vec::<u32>::new());
        assert_eq!(keys(idx.query_point(Point::new(Nm(13), Nm(5)), Nm(3))), Vec::<u32>::new());
        assert_eq!(keys(idx.query_point(Point::new(Nm(17), Nm(5)), Nm(3))), [2]);
        assert_eq!(idx.nearest(Point::new(Nm(95), Nm(100))).map(|(k, _)| k), Some(1));
        assert_eq!(keys(idx.nearest_iter(Point::ORIGIN).take(1)), [2]);
        assert_eq!(idx.remove(2), Some(r(20, 0, 30, 10)));
        assert_eq!(idx.remove(2), None);
        assert_eq!(keys(idx.iter()), [1]);
        idx.clear();
        assert!(idx.is_empty());
        assert_eq!(idx.nearest(Point::ORIGIN), None);
    }
}
