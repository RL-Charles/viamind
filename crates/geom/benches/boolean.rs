//! Spike T-0018: polygon booleans and offsets, `i_overlay` (integer API) vs `geo`.
//!
//! The scene is a copper pour on a 100 × 80 mm board: about 50k obstacle
//! vertices (vias, track capsules, rectangular pads) that the pour must clear.
//! Each library runs the same operations:
//!
//! - **union**: merge all obstacles into one region set
//! - **difference**: board outline minus the (overlapping) obstacles
//! - **offset**: grow the obstacles by a 0.2 mm clearance with round joins
//! - **pour**: offset then difference, i.e. a full pour fill
//! - **pour pre-inflated**: the same pour with each obstacle grown by the
//!   clearance analytically (bigger radius, rounded corners) before flattening,
//!   so only the difference runs
//!
//! Run with `cargo bench -p viamind-geom --bench boolean`. Results and the
//! decision are in `docs/decisions/0002-polygon-booleans.md`.

use std::f64::consts::TAU;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use geo::algorithm::bool_ops::FillRule as GeoFillRule;
use geo::algorithm::buffer::{BufferStyle, LineJoin as GeoLineJoin};
use geo::{Area, BooleanOps, Buffer, Coord, LineString, MultiPolygon, unary_union};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::core::single::SingleIntOverlay;
use i_overlay::i_float::int::angle::Angle as IntAngle;
use i_overlay::i_float::int::number::int::IntNumber;
use i_overlay::i_float::int::point::IntPoint;
use i_overlay::mesh::int::arc::ArcOptions;
use i_overlay::mesh::int::outline::offset::IntOutlineOffset;
use i_overlay::mesh::int::style::{IntLineJoin, IntOutlineStyle};
use viamind_geom::{Angle, Arc, Capsule, Circle, Nm, Point, Polygon, Rect, Segment, Shape, Vector};

/// Pour clearance.
const CLEARANCE_NM: i64 = 200_000;
/// Arc step for round joins in both libraries: 1/200 turn (1.8°), the finest `geo` allows
/// (about 25 nm of chord error at 0.2 mm). Angles are in i_overlay units of 2^-32 turn.
const JOIN_STEP: u32 = (1u64 << 32).div_ceil(200) as u32;
/// The finest step i_overlay allows, 1/1024 turn (0.35°, about 1 nm of chord error at 0.2 mm).
const FINE_JOIN_STEP: u32 = 1 << 22;

/// Small deterministic generator (xorshift64*), so runs are repeatable without extra dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn mm(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }
}

struct Scene {
    outline: Polygon,
    /// Obstacles as drawn, flattened to polygons.
    obstacles: Vec<Polygon>,
    /// The same obstacles grown by the clearance before flattening (circles and
    /// capsules get a larger radius, rectangles get rounded corners).
    inflated: Vec<Polygon>,
}

/// A rectangle grown by `d` with round corners, counter-clockwise.
fn rounded_rect(r: &Rect, d: Nm, tol: Nm) -> Polygon {
    let corners = [(r.max.x, r.min.y), (r.max.x, r.max.y), (r.min.x, r.max.y), (r.min.x, r.min.y)];
    let mut pts = Vec::new();
    for (i, (x, y)) in corners.into_iter().enumerate() {
        let start = Angle(Angle::QUARTER.0 * (i as i64 - 1));
        pts.extend(Arc::new(Point::new(x, y), d, start, Angle::QUARTER).flatten(tol));
    }
    Polygon::new(pts)
}

/// A 100 × 80 mm board with vias, tracks and pads scattered over it.
fn scene() -> Scene {
    let mut rng = Rng(0x5eed);
    let outline = Polygon::from_rect(&Rect::new(Point::ORIGIN, Point::from_mm(100.0, 80.0)));
    let tol = Nm::from_um(1.5);
    let clearance = Nm(CLEARANCE_NM);
    let (mut obstacles, mut inflated) = (Vec::new(), Vec::new());
    for _ in 0..1100 {
        let via = Circle::new(Point::from_mm(rng.mm(1.0, 99.0), rng.mm(1.0, 79.0)), Nm::from_mm(0.3));
        obstacles.push(Shape::from(via).to_polygon(tol));
        inflated.push(Shape::from(via.inflate(clearance)).to_polygon(tol));
    }
    for _ in 0..400 {
        let a = Point::from_mm(rng.mm(5.0, 95.0), rng.mm(5.0, 75.0));
        let (len, ang) = (rng.mm(1.0, 10.0), rng.unit() * TAU);
        let b = a + Vector::from_mm(len * ang.cos(), len * ang.sin());
        let track = Capsule::new(Segment::new(a, b), Nm::from_mm(0.2));
        obstacles.push(Shape::from(track).to_polygon(tol));
        inflated.push(Shape::from(track.inflate(clearance)).to_polygon(tol));
    }
    for _ in 0..1500 {
        let p = Point::from_mm(rng.mm(1.0, 97.0), rng.mm(1.0, 77.0));
        let pad = Rect::new(p, p + Vector::from_mm(rng.mm(0.5, 2.0), rng.mm(0.5, 2.0)));
        obstacles.push(Polygon::from_rect(&pad));
        inflated.push(rounded_rect(&pad, clearance, tol));
    }
    Scene { outline, obstacles, inflated }
}

// ---- i_overlay integer API ----

fn to_int<I: IntNumber + TryFrom<i64>>(poly: &Polygon) -> Vec<IntPoint<I>>
where
    I::Error: std::fmt::Debug,
{
    poly.points.iter().map(|p| IntPoint::new(I::try_from(p.x.0).unwrap(), I::try_from(p.y.0).unwrap())).collect()
}

/// Shapes as returned by i_overlay: each shape is an outer contour then its holes.
type IntShapes<I> = Vec<Vec<Vec<IntPoint<I>>>>;

macro_rules! i_overlay_ops {
    ($m:ident, $int:ty) => {
        mod $m {
            use super::*;

            pub fn union(obs: &[Vec<IntPoint<$int>>]) -> IntShapes<$int> {
                Overlay::from_subj(obs).overlay(OverlayRule::Subject, FillRule::NonZero)
            }

            pub fn difference(outline: &[IntPoint<$int>], obs: &[Vec<IntPoint<$int>>]) -> IntShapes<$int> {
                outline.overlay(obs, OverlayRule::Difference, FillRule::NonZero)
            }

            pub fn offset(obs: &[Vec<IntPoint<$int>>]) -> IntShapes<$int> {
                offset_with_step(obs, JOIN_STEP)
            }

            pub fn offset_with_step(obs: &[Vec<IntPoint<$int>>], step: u32) -> IntShapes<$int> {
                let join = IntLineJoin::Round(ArcOptions { max_step: IntAngle::from_bits(step), rotation_precision: 5 });
                let style = IntOutlineStyle::new(CLEARANCE_NM as $int).line_join(join);
                obs.outline(&style).unwrap()
            }

            pub fn pour(outline: &[IntPoint<$int>], obs: &[Vec<IntPoint<$int>>]) -> IntShapes<$int> {
                let grown = offset(obs);
                let contours: Vec<&[IntPoint<$int>]> = grown.iter().flatten().map(Vec::as_slice).collect();
                outline.overlay(contours.as_slice(), OverlayRule::Difference, FillRule::NonZero)
            }
        }
    };
}

i_overlay_ops!(int64, i64);
i_overlay_ops!(int32, i32);

fn int_stats<I: IntNumber + Into<i64>>(shapes: &IntShapes<I>) -> (usize, f64) {
    let mut verts = 0;
    let mut area2: i128 = 0;
    for contour in shapes.iter().flatten() {
        verts += contour.len();
        let poly = Polygon::new(contour.iter().map(|p| Point::new(Nm(p.x.into()), Nm(p.y.into()))).collect());
        area2 += poly.signed_area_x2();
    }
    (verts, area2 as f64 / 2.0)
}

// ---- geo (f64) ----

fn to_geo(poly: &Polygon) -> geo::Polygon<f64> {
    let ring: Vec<Coord<f64>> = poly.points.iter().map(|p| Coord { x: p.x.0 as f64, y: p.y.0 as f64 }).collect();
    geo::Polygon::new(LineString::new(ring), vec![])
}

mod geo_ops {
    use super::*;

    pub fn union(obs: &[geo::Polygon<f64>]) -> MultiPolygon<f64> {
        unary_union(obs)
    }

    pub fn difference(outline: &geo::Polygon<f64>, obs: &MultiPolygon<f64>) -> MultiPolygon<f64> {
        outline.difference_with_fill_rule(obs, GeoFillRule::NonZero)
    }

    pub fn offset(obs: &MultiPolygon<f64>) -> MultiPolygon<f64> {
        let step = TAU * f64::from(JOIN_STEP) / 2f64.powi(32);
        obs.buffer_with_style(BufferStyle::new(CLEARANCE_NM as f64).line_join(GeoLineJoin::Round(step)))
    }

    pub fn pour(outline: &geo::Polygon<f64>, obs: &MultiPolygon<f64>) -> MultiPolygon<f64> {
        outline.difference(&offset(obs))
    }
}

fn geo_stats(mp: &MultiPolygon<f64>) -> (usize, f64) {
    let verts = mp.iter().map(|p| p.exterior().0.len() - 1 + p.interiors().iter().map(|r| r.0.len() - 1).sum::<usize>()).sum();
    (verts, mp.unsigned_area())
}

fn benches(c: &mut Criterion) {
    let scene = scene();
    let obstacle_verts: usize = scene.obstacles.iter().map(|p| p.points.len()).sum();

    let outline64 = to_int::<i64>(&scene.outline);
    let obs64: Vec<_> = scene.obstacles.iter().map(to_int::<i64>).collect();
    let outline32 = to_int::<i32>(&scene.outline);
    let obs32: Vec<_> = scene.obstacles.iter().map(to_int::<i32>).collect();
    let outline_geo = to_geo(&scene.outline);
    let obs_geo: Vec<_> = scene.obstacles.iter().map(to_geo).collect();
    let obs_geo_mp = MultiPolygon::new(obs_geo.clone());
    let inflated_verts: usize = scene.inflated.iter().map(|p| p.points.len()).sum();
    let inf64: Vec<_> = scene.inflated.iter().map(to_int::<i64>).collect();
    let inf32: Vec<_> = scene.inflated.iter().map(to_int::<i32>).collect();
    let inf_geo = MultiPolygon::new(scene.inflated.iter().map(to_geo).collect());

    // Cross-check: both libraries agree on the result before we time them.
    eprintln!("scene: {} obstacles, {obstacle_verts} vertices", scene.obstacles.len());
    let report = |op: &str, lib: &str, (verts, area): (usize, f64)| {
        eprintln!("  {op:<10} {lib:<13} {verts:>7} output vertices, area {:.6} mm²", area / 1e12);
    };
    report("union", "i_overlay i64", int_stats(&int64::union(&obs64)));
    report("union", "i_overlay i32", int_stats(&int32::union(&obs32)));
    report("union", "geo", geo_stats(&geo_ops::union(&obs_geo)));
    report("difference", "i_overlay i64", int_stats(&int64::difference(&outline64, &obs64)));
    report("difference", "geo", geo_stats(&geo_ops::difference(&outline_geo, &obs_geo_mp)));
    report("offset", "i_overlay i64", int_stats(&int64::offset(&obs64)));
    report("offset", "geo", geo_stats(&geo_ops::offset(&obs_geo_mp)));
    report("offset", "i_ov i32 fine", int_stats(&int32::offset_with_step(&obs32, FINE_JOIN_STEP)));
    report("pour", "i_overlay i64", int_stats(&int64::pour(&outline64, &obs64)));
    report("pour", "i_overlay i32", int_stats(&int32::pour(&outline32, &obs32)));
    report("pour", "geo", geo_stats(&geo_ops::pour(&outline_geo, &obs_geo_mp)));
    eprintln!("pre-inflated obstacles: {inflated_verts} vertices");
    report("pour pre", "i_overlay i64", int_stats(&int64::difference(&outline64, &inf64)));
    report("pour pre", "i_overlay i32", int_stats(&int32::difference(&outline32, &inf32)));
    report("pour pre", "geo", geo_stats(&geo_ops::difference(&outline_geo, &inf_geo)));
    let ours = viamind_geom::boolean::difference([&scene.outline], &scene.inflated);
    let ours_verts = ours.iter().flat_map(viamind_geom::Region::rings).map(|r| r.points.len()).sum();
    report("pour pre", "viamind", (ours_verts, ours.iter().map(viamind_geom::Region::area).sum()));

    let mut g = c.benchmark_group(format!("pour {obstacle_verts} vertices"));
    g.sample_size(20);
    g.bench_function(BenchmarkId::new("union", "i_overlay i64"), |b| b.iter(|| black_box(int64::union(&obs64))));
    g.bench_function(BenchmarkId::new("union", "i_overlay i32"), |b| b.iter(|| black_box(int32::union(&obs32))));
    g.bench_function(BenchmarkId::new("union", "geo"), |b| b.iter(|| black_box(geo_ops::union(&obs_geo))));
    g.bench_function(BenchmarkId::new("difference", "i_overlay i64"), |b| b.iter(|| black_box(int64::difference(&outline64, &obs64))));
    g.bench_function(BenchmarkId::new("difference", "i_overlay i32"), |b| b.iter(|| black_box(int32::difference(&outline32, &obs32))));
    g.bench_function(BenchmarkId::new("difference", "geo"), |b| b.iter(|| black_box(geo_ops::difference(&outline_geo, &obs_geo_mp))));
    g.bench_function(BenchmarkId::new("offset", "i_overlay i64"), |b| b.iter(|| black_box(int64::offset(&obs64))));
    g.bench_function(BenchmarkId::new("offset", "i_overlay i32"), |b| b.iter(|| black_box(int32::offset(&obs32))));
    g.bench_function(BenchmarkId::new("offset", "geo"), |b| b.iter(|| black_box(geo_ops::offset(&obs_geo_mp))));
    g.bench_function(BenchmarkId::new("offset", "i_overlay i32 0.35°"), |b| {
        b.iter(|| black_box(int32::offset_with_step(&obs32, FINE_JOIN_STEP)));
    });
    g.bench_function(BenchmarkId::new("pour", "i_overlay i64"), |b| b.iter(|| black_box(int64::pour(&outline64, &obs64))));
    g.bench_function(BenchmarkId::new("pour", "i_overlay i32"), |b| b.iter(|| black_box(int32::pour(&outline32, &obs32))));
    g.bench_function(BenchmarkId::new("pour", "geo"), |b| b.iter(|| black_box(geo_ops::pour(&outline_geo, &obs_geo_mp))));
    g.bench_function(BenchmarkId::new("pour pre-inflated", "i_overlay i64"), |b| b.iter(|| black_box(int64::difference(&outline64, &inf64))));
    g.bench_function(BenchmarkId::new("pour pre-inflated", "i_overlay i32"), |b| b.iter(|| black_box(int32::difference(&outline32, &inf32))));
    g.bench_function(BenchmarkId::new("pour pre-inflated", "geo"), |b| b.iter(|| black_box(geo_ops::difference(&outline_geo, &inf_geo))));
    g.bench_function(BenchmarkId::new("pour pre-inflated", "viamind boolean"), |b| {
        b.iter(|| black_box(viamind_geom::boolean::difference([&scene.outline], &scene.inflated)));
    });
    g.finish();
}

criterion_group!(boolean, benches);
criterion_main!(boolean);
