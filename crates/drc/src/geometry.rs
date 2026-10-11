//! Ephemeral oriented copper geometry in nanometres; no persistent float model.
use viamind_doc::{Part, Side};
use viamind_geom::{Nm, Point, Rect};

pub(crate) const EPS: f64 = 0.001;
pub(crate) type P = [f64; 2];
pub(crate) fn point(p: Point) -> P {
    [p.x.0 as f64, p.y.0 as f64]
}
pub(crate) fn bounds(points: &[P]) -> Rect {
    let min = |axis| points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min).floor() as i64;
    let max = |axis| points.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max).ceil() as i64;
    Rect::new(Point::new(Nm(min(0)), Nm(min(1))), Point::new(Nm(max(0)), Nm(max(1))))
}
pub(crate) fn rectangle(part: &Part, pad: &viamind_doc::Pad) -> [P; 4] {
    let (s, c) = match part.rotation_mdeg {
        0 => (0.0, 1.0),
        90_000 => (1.0, 0.0),
        180_000 => (0.0, -1.0),
        270_000 => (-1.0, 0.0),
        angle => (f64::from(angle) / 1000.0).to_radians().sin_cos(),
    };
    let x = pad.position.x.0 as f64;
    let y = pad.position.y.0 as f64;
    let w = pad.width.0 as f64 / 2.0;
    let h = pad.height.0 as f64 / 2.0;
    [[x - w, y - h], [x + w, y - h], [x + w, y + h], [x - w, y + h]].map(|[x, y]| {
        let x = if part.side == Side::Bottom { -x } else { x };
        [part.position.x.0 as f64 + x * c + y * s, part.position.y.0 as f64 - x * s + y * c]
    })
}
fn cross(a: P, b: P, p: P) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn length(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
pub(crate) fn edges(polygon: &[P]) -> impl Iterator<Item = (P, P)> + '_ {
    (0..polygon.len()).map(|i| (polygon[i], polygon[(i + 1) % polygon.len()]))
}
fn point_distance(p: P, a: P, b: P) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let square = dx * dx + dy * dy;
    if square == 0.0 {
        return length(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / square).clamp(0.0, 1.0);
    length(p, [a[0] + t * dx, a[1] + t * dy])
}
pub(crate) fn segment_distance(a: P, b: P, c: P, d: P) -> f64 {
    let opposite = |a: f64, b: f64| (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0);
    if opposite(cross(a, b, c), cross(a, b, d)) && opposite(cross(c, d, a), cross(c, d, b)) {
        return 0.0;
    }
    point_distance(a, c, d).min(point_distance(b, c, d)).min(point_distance(c, a, b)).min(point_distance(d, a, b))
}
pub(crate) fn contains(polygon: &[P], p: P, include_boundary: bool) -> bool {
    let mut inside = false;
    for (a, b) in edges(polygon) {
        if point_distance(p, a, b) <= EPS {
            return include_boundary;
        }
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}
pub(crate) fn rectangle_distance(a: &[P; 4], b: &[P; 4]) -> f64 {
    if contains(a, b[0], true) || contains(b, a[0], true) {
        return 0.0;
    }
    edges(a).flat_map(|(u, v)| edges(b).map(move |(c, d)| segment_distance(u, v, c, d))).fold(f64::INFINITY, f64::min)
}
/// Whether any open segment interval enters the strict interior of a rectangle.
/// This detects a concave board notch or a wholly enclosed cutout between pad corners.
pub(crate) fn enters_rectangle(rect: &[P; 4], a: P, b: P) -> bool {
    let sign = cross(rect[0], rect[1], rect[2]).signum();
    let mut lo: f64 = 0.0;
    let mut hi: f64 = 1.0;
    for (u, v) in edges(rect) {
        let size = length(u, v);
        let f0 = sign * cross(u, v, a) / size;
        let df = sign * cross(u, v, b) / size - f0;
        if df == 0.0 {
            if f0 <= EPS {
                return false;
            }
        } else if df > 0.0 {
            lo = lo.max((EPS - f0) / df);
        } else {
            hi = hi.min((EPS - f0) / df);
        }
        if hi <= lo {
            return false;
        }
    }
    hi > lo
}
