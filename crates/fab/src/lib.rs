//! Manufacturing exports derived from immutable design snapshots.
//!
//! Generic assembly CSV preserves component origins and model text. It does not
//! infer assembly centroids, sourcing identifiers or manufacturer compatibility.

mod assembly;
mod fabrication;

pub use assembly::{AssemblyCsv, csv, write_csv};
pub use fabrication::{FabricationManifest, FabricationOptions, FabricationPackage, fabrication, fabrication_schema, write_fabrication};
