//! Property tests: the spatial index agrees with a brute-force scan.

use std::collections::HashMap;

use proptest::prelude::*;
use viamind_geom::{Nm, Point, Rect, SpatialIndex};

const SPAN: i64 = 1_000_000;

fn pt() -> impl Strategy<Value = Point> {
    (-SPAN..=SPAN, -SPAN..=SPAN).prop_map(|(x, y)| Point::new(Nm(x), Nm(y)))
}

fn rect() -> impl Strategy<Value = Rect> {
    (pt(), 0i64..SPAN / 5, 0i64..SPAN / 5).prop_map(|(p, w, h)| Rect::new(p, Point::new(p.x + Nm(w), p.y + Nm(h))))
}

#[derive(Clone, Debug)]
enum Op {
    Insert(u16, Rect),
    Update(u16, Rect),
    Remove(u16),
}

fn op() -> impl Strategy<Value = Op> {
    let key = 0u16..64;
    prop_oneof![
        3 => (key.clone(), rect()).prop_map(|(k, r)| Op::Insert(k, r)),
        1 => (key.clone(), rect()).prop_map(|(k, r)| Op::Update(k, r)),
        1 => key.prop_map(Op::Remove),
    ]
}

fn sorted(it: impl Iterator<Item = (u16, Rect)>) -> Vec<u16> {
    let mut v: Vec<u16> = it.map(|(k, _)| k).collect();
    v.sort_unstable();
    v
}

proptest! {
    #[test]
    fn index_matches_brute_force(ops in prop::collection::vec(op(), 0..200), area in rect(), p in pt(), tol in 0i64..SPAN / 10) {
        let mut index = SpatialIndex::new();
        let mut model: HashMap<u16, Rect> = HashMap::new();
        for op in ops {
            match op {
                Op::Insert(k, r) => prop_assert_eq!(index.insert(k, r), model.insert(k, r)),
                Op::Update(k, r) => {
                    let expected = model.get_mut(&k).map(|slot| std::mem::replace(slot, r));
                    prop_assert_eq!(index.update(k, r), expected);
                }
                Op::Remove(k) => prop_assert_eq!(index.remove(k), model.remove(&k)),
            }
        }
        prop_assert_eq!(index.len(), model.len());
        prop_assert_eq!(sorted(index.iter()), sorted(model.iter().map(|(&k, &r)| (k, r))));

        let in_area = sorted(model.iter().filter(|(_, r)| r.intersects(&area)).map(|(&k, &r)| (k, r)));
        prop_assert_eq!(sorted(index.query_rect(&area)), in_area);

        let near = sorted(model.iter().filter(|(_, r)| r.distance_to_point(p) <= tol as f64).map(|(&k, &r)| (k, r)));
        prop_assert_eq!(sorted(index.query_point(p, Nm(tol))), near);

        let best = model.values().map(|r| r.distance_to_point(p)).fold(f64::INFINITY, f64::min);
        match index.nearest(p) {
            Some((_, r)) => prop_assert_eq!(r.distance_to_point(p), best),
            None => prop_assert!(model.is_empty()),
        }

        let rebuilt = SpatialIndex::bulk_load(model.iter().map(|(&k, &r)| (k, r)));
        prop_assert_eq!(sorted(rebuilt.query_rect(&area)), sorted(index.query_rect(&area)));
    }
}
