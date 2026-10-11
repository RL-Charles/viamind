//! Manufacturing exports derived from immutable design snapshots.
//!
//! Generic assembly CSV preserves component origins and model text. It does not
//! infer assembly centroids, sourcing identifiers or manufacturer compatibility.

mod assembly;

pub use assembly::{AssemblyCsv, csv, write_csv};
