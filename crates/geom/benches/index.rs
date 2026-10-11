//! Spatial index benchmarks on a dense synthetic board of one million objects.
//!
//! Run with `cargo bench -p viamind-geom --bench index`.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use viamind_geom::{Nm, Point, Rect, SpatialIndex, Vector};

const OBJECTS: usize = 1_000_000;
/// A 500 mm × 500 mm board.
const BOARD_MM: f64 = 500.0;

/// Small deterministic generator (xorshift64*), so runs are repeatable without extra dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn point(&mut self) -> Point {
        Point::from_mm(self.unit() * BOARD_MM, self.unit() * BOARD_MM)
    }

    /// Object boxes from 0.05 mm (vias, small pads) to 5 mm (track segments, parts).
    fn rect(&mut self) -> Rect {
        let p = self.point();
        let size = Vector::from_mm(0.05 + self.unit() * 4.95, 0.05 + self.unit() * 4.95);
        Rect::new(p, p + size)
    }
}

fn board() -> SpatialIndex<u32> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    SpatialIndex::bulk_load((0..OBJECTS as u32).map(|k| (k, rng.rect())))
}

fn benches(c: &mut Criterion) {
    let index = board();
    let mut rng = Rng(42);
    let probes: Vec<Point> = (0..4096).map(|_| rng.point()).collect();
    let mut i = 0;
    let mut next_probe = || {
        i = (i + 1) % probes.len();
        probes[i]
    };

    c.bench_function("hit test 1M objects (0.1 mm pick radius)", |b| {
        b.iter(|| {
            let hits: Vec<(u32, Rect)> = index.query_point(next_probe(), Nm::from_mm(0.1)).collect();
            black_box(hits)
        });
    });

    c.bench_function("query 10 mm window, 1M objects", |b| {
        b.iter(|| {
            let p = next_probe();
            let area = Rect::new(p, p + Vector::from_mm(10.0, 10.0));
            black_box(index.query_rect(&area).count())
        });
    });

    c.bench_function("nearest, 1M objects", |b| b.iter(|| black_box(index.nearest(next_probe()))));

    c.bench_function("move one object, 1M objects", |b| {
        let mut index = index.clone();
        let mut k = 0u32;
        b.iter(|| {
            k = (k + 7919) % OBJECTS as u32;
            let r = index.get(k).unwrap_or(Rect::from_point(Point::ORIGIN));
            black_box(index.update(k, r.translate(Vector::from_mm(0.1, 0.0))))
        });
    });

    let mut group = c.benchmark_group("build");
    group.sample_size(10);
    group.bench_function("bulk load 1M objects", |b| {
        b.iter_batched(
            || {
                let mut rng = Rng(7);
                (0..OBJECTS as u32).map(|k| (k, rng.rect())).collect::<Vec<_>>()
            },
            |items| black_box(SpatialIndex::bulk_load(items)),
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(index, benches);
criterion_main!(index);
