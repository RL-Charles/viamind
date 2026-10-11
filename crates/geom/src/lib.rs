//! Geometry primitives, integer nanometre units and spatial indexing for Viamind.
//!
//! Coordinates and lengths are integer nanometres ([`Nm`]) and angles integer
//! nanodegrees ([`Angle`]), so stored geometry is exact. Predicates such as
//! segment intersection and point-in-polygon are exact too (computed in `i128`).
//! Measured distances are returned as `f64` nanometres, accurate to well under a
//! nanometre for board-sized coordinates.
//!
//! ```
//! use viamind_geom::{Capsule, Circle, Nm, Point, Segment, Shape};
//!
//! let via = Shape::from(Circle::new(Point::from_mm(0.0, 0.0), Nm::from_mm(0.3)));
//! let track = Shape::from(Capsule::new(Segment::new(Point::from_mm(-5.0, 1.0), Point::from_mm(5.0, 1.0)), Nm::from_mm(0.2)));
//! assert!((via.distance(&track) - Nm::from_mm(0.6).0 as f64).abs() < 1.0);
//! ```

pub mod angle;
pub mod arc;
pub mod point;
pub mod polygon;
pub mod rect;
pub mod segment;
pub mod shape;
pub mod units;

pub use angle::{Angle, NANODEG_PER_DEG};
pub use arc::Arc;
pub use point::{Point, Vector};
pub use polygon::Polygon;
pub use rect::Rect;
pub use segment::{Segment, SegmentIntersection};
pub use shape::{Capsule, Circle, Shape};
pub use units::{NM_PER_INCH, NM_PER_MIL, NM_PER_MM, NM_PER_UM, Nm};
