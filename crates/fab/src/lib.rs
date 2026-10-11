//! Manufacturing exports derived from immutable design snapshots.
//!
//! Generic assembly CSV preserves component origins and model text. It does not
//! infer assembly centroids, sourcing identifiers or manufacturer compatibility.

mod assembly;
mod assembly_placement;
mod assembly_templates;
mod fabrication;
mod output_job;
mod populated_bom;
mod release_inspection;
mod snapshot;

// Fixture exporters and admission tests share the same production permit pool.
// Serialize these tests together so intentional saturation has no unrelated users.
#[cfg(test)]
static MANUFACTURING_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub use assembly::{AssemblyCsv, AssemblyExportRequest, AssemblySnapshot, PreparedAssemblyCsv, assembly_schema, csv, write_csv};
pub use fabrication::{
    FabricationExportRequest, FabricationManifest, FabricationOptions, FabricationPackage, FootprintArtworkSummary, PadApertureSummary, fabrication,
    fabrication_schema, write_fabrication,
};
pub use snapshot::FabricationSnapshot;

pub use output_job::{
    CHECKED_PROFILE, CalibratedPlacementSummary, OutputJob, OutputJobChecks, OutputJobFileRequest, OutputJobSnapshot, PopulatedAssemblySummary,
    ReleaseManifest, RunOutputJobRequest, SaveOutputJobRequest, load_output_job, output_job_schema, save_output_job,
};

pub use release_inspection::{
    InspectOutputRelease, MatchedAssembly, ReleaseCursor, ReleaseFileFact, ReleaseInspection, inspect_output_release, release_inspection_schema,
};
