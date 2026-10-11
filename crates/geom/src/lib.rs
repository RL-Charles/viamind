//! Geometry primitives, integer nanometre units and spatial indexing for Viamind

pub mod point;
pub mod units;

pub use point::{Point, Vector};
pub use units::{NM_PER_INCH, NM_PER_MIL, NM_PER_MM, NM_PER_UM, Nm};
