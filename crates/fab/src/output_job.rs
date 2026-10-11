//! Portable bounded manufacturing jobs. One board capture feeds every release file.
use crate::{AssemblyCsv, FabricationOptions, FabricationSnapshot, assembly::render_csv_limited, snapshot::invalid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use viamind_doc::Project;
const CONFIG_LIMIT: usize = 64 * 1024;
const RELEASE_LIMIT: usize = 192 * 1024 * 1024;
const VALIDATION_LIMIT: usize = 16 * 1024 * 1024;
pub const CHECKED_PROFILE: &str = "checked_outer_2layer";

/// Explicit saved request baselines; document minima are resolved centrally by DRC.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputJobChecks {
    pub copper_clearance_mm: f64,
    pub edge_clearance_mm: f64,
    pub minimum_via_drill_mm: f64,
    pub minimum_annular_width_mm: f64,
    pub hole_clearance_mm: f64,
    pub hole_to_copper_clearance_mm: f64,
    pub max_violations: usize,
}
impl OutputJobChecks {
    pub fn validate(&self) -> io::Result<()> {
        for value in [
            self.copper_clearance_mm,
            self.edge_clearance_mm,
            self.minimum_via_drill_mm,
            self.minimum_annular_width_mm,
            self.hole_clearance_mm,
            self.hole_to_copper_clearance_mm,
        ] {
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(invalid("checked job distance baselines must be finite within0..100mm"));
            }
        }
        if !(1..=10000).contains(&self.max_violations) {
            return Err(invalid("checked job max_violations must be within1..10000"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputJob {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub profile: String,
    #[serde(default)]
    pub mask_expansion_mm: f64,
    #[serde(default, deserialize_with = "present_checks", skip_serializing_if = "Option::is_none")]
    pub checks: Option<OutputJobChecks>,
}
fn present_checks<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<OutputJobChecks>, D::Error> {
    OutputJobChecks::deserialize(deserializer).map(Some)
}
impl OutputJob {
    pub fn new(name: String, mask_expansion_mm: f64) -> Self {
        Self { format: "viamind-output-job".into(), version: 1, name, profile: "generic_outer_2layer".into(), mask_expansion_mm, checks: None }
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.format != "viamind-output-job" || self.version != 1 || !matches!(self.profile.as_str(), "generic_outer_2layer" | CHECKED_PROFILE) {
            return Err(invalid("unsupported output job format, version or profile"));
        }
        match (self.profile.as_str(), &self.checks) {
            (CHECKED_PROFILE, Some(checks)) => checks.validate()?,
            ("generic_outer_2layer", None) => {}
            _ => return Err(invalid("checked profile requires explicit checks; generic profile forbids checks")),
        }
        if self.name.is_empty() || self.name.len() > 256 || self.name.trim().is_empty() || self.name.chars().any(char::is_control) {
            return Err(invalid("job name needs 1–256 UTF-8 bytes, nonblank and without control characters"));
        }
        if !self.mask_expansion_mm.is_finite() || !(0.0..=10.0).contains(&self.mask_expansion_mm) {
            return Err(invalid("mask expansion needs a finite value from 0 to 10 mm"));
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > CONFIG_LIMIT {
            return Err(invalid("output job exceeds 64 KiB"));
        }
        let job: Self = serde_json::from_slice(bytes).map_err(invalid_json)?;
        job.validate()?;
        Ok(job)
    }
    pub fn canonical_bytes(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        bounded_json(self, CONFIG_LIMIT)
    }
}
fn invalid_json(error: serde_json::Error) -> io::Error {
    invalid(error.to_string())
}
/// Linux opens do not follow symlinks or block on FIFO/device paths.
pub fn load_output_job(path: &Path) -> io::Result<OutputJob> {
    validate_path(path)?;
    #[cfg(target_os = "linux")]
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    #[cfg(not(target_os = "linux"))]
    let file = {
        let _ = path;
        return Err(io::Error::new(io::ErrorKind::Unsupported, "output jobs initially support Linux"));
    };
    if !file.metadata()?.is_file() {
        return Err(invalid("output job must be a regular non-symlink file"));
    }
    let mut bytes = Vec::new();
    file.take((CONFIG_LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    OutputJob::parse(&bytes)
}
fn validate_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty() || path.as_os_str().len() > 4096 {
        return Err(invalid("path needs 1–4096 bytes"));
    }
    Ok(())
}
fn destination(path: &Path, native: Option<&Path>) -> io::Result<PathBuf> {
    validate_path(path)?;
    let name = path.file_name().filter(|n| !n.is_empty()).ok_or_else(|| invalid("destination needs a name"))?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).canonicalize()?;
    let result = parent.join(name);
    if let Some(native) = native {
        let native = native.canonicalize().unwrap_or(std::path::absolute(native)?);
        if result == native || result.canonicalize().is_ok_and(|p| p == native) {
            return Err(invalid("output must differ from active native project"));
        }
    }
    if result.symlink_metadata().is_ok() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "output destination already exists"));
    }
    Ok(result)
}
pub fn save_output_job(job: &OutputJob, path: &Path, native: Option<&Path>) -> io::Result<PathBuf> {
    let bytes = job.canonical_bytes()?;
    let path = destination(path, native)?;
    let mut stage = tempfile::Builder::new().prefix(".viamind-job-").tempfile_in(path.parent().expect("canonical parent"))?;
    stage.write_all(&bytes)?;
    stage.as_file().sync_all()?;
    stage.persist_noclobber(&path).map_err(|e| e.error)?;
    Ok(path)
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputJobFileRequest {
    pub path: PathBuf,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveOutputJobRequest {
    pub path: PathBuf,
    pub job: OutputJob,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, bound(deserialize = "C: Deserialize<'de>"))]
pub struct RunOutputJobRequest<C> {
    pub job_path: PathBuf,
    pub path: PathBuf,
    #[serde(default)]
    pub expected_cursor: Option<C>,
}
pub fn output_job_schema(action: &str) -> Value {
    let path = json!({"type":"string","minLength":1});
    let distance = json!({"type":"number","minimum":0,"maximum":100});
    let checks = json!({"type":"object","additionalProperties":false,
        "required":["copper_clearance_mm","edge_clearance_mm","minimum_via_drill_mm","minimum_annular_width_mm","hole_clearance_mm","hole_to_copper_clearance_mm","max_violations"],
        "properties":{"copper_clearance_mm":distance,"edge_clearance_mm":distance,"minimum_via_drill_mm":distance,"minimum_annular_width_mm":distance,"hole_clearance_mm":distance,"hole_to_copper_clearance_mm":distance,"max_violations":{"type":"integer","minimum":1,"maximum":10000}}});
    match action {
        "load" => json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{"path":path}}),
        "save" => json!({"type":"object","additionalProperties":false,"required":["path","job"],"properties":{"path":path,"job":{
            "type":"object","additionalProperties":false,"required":["format","version","name","profile"],
            "properties":{"format":{"const":"viamind-output-job"},"version":{"const":1},"name":{"type":"string","minLength":1,"maxLength":256},"profile":{"enum":["generic_outer_2layer",CHECKED_PROFILE]},"mask_expansion_mm":{"type":"number","minimum":0,"maximum":10,"default":0},"checks":checks},
            "oneOf":[{"properties":{"profile":{"const":"generic_outer_2layer"}},"not":{"required":["checks"]}},{"properties":{"profile":{"const":CHECKED_PROFILE}},"required":["checks"]}]
        }}}),
        _ => {
            json!({"type":"object","additionalProperties":false,"required":["job_path","path"],"properties":{"job_path":path,"path":path,"expected_cursor":{"type":["object","null"],"additionalProperties":false,"required":["stream_id","sequence"],"properties":{"stream_id":{"type":"string","format":"uuid"},"sequence":{"type":"integer","minimum":0}}}}})
        }
    }
}
/// Holds exactly one shared manufacturing admission slot until publication/drop.
pub struct OutputJobSnapshot {
    board: FabricationSnapshot,
    job: OutputJob,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseManifest<C> {
    pub format: String,
    pub version: u32,
    pub project_id: String,
    pub exported_cursor: C,
    /// Relative file paths and byte lengths; excludes this manifest itself.
    pub files: BTreeMap<String, usize>,
    pub exclusions: Vec<String>,
}
impl OutputJobSnapshot {
    pub fn capture(project: &Project, job: OutputJob) -> io::Result<Self> {
        job.validate()?;
        if project.board.parts.len() > 100_000 {
            return Err(invalid("output jobs support at most 100000 components"));
        }
        Ok(Self { board: FabricationSnapshot::capture(project)?, job })
    }
    pub fn write<C: Serialize>(self, cursor: C, path: &Path, native: Option<&Path>) -> io::Result<(PathBuf, ReleaseManifest<C>)> {
        if self.job.checks.is_some() {
            return Err(invalid("checked profile requires the production validation callback"));
        }
        self.write_with_before_publish(cursor, path, native, || Ok(()))
    }
    fn write_with_before_publish<C: Serialize>(
        self,
        cursor: C,
        path: &Path,
        native: Option<&Path>,
        before_publish: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<(PathBuf, ReleaseManifest<C>)> {
        self.write_impl(cursor, path, native, |_, _, _| Err(invalid("checked profile requires the production validation callback")), before_publish)
    }
    /// Internal application bridge: callbacks must run the trusted production validator.
    /// No caller-supplied report or pass flag is accepted by CLI/MCP/desktop requests.
    pub fn write_with_validation<C: Serialize>(
        self,
        cursor: C,
        path: &Path,
        native: Option<&Path>,
        validate: impl FnOnce(&Project, &Value, &C) -> io::Result<Vec<u8>>,
    ) -> io::Result<(PathBuf, ReleaseManifest<C>)> {
        self.write_impl(cursor, path, native, validate, || Ok(()))
    }
    fn write_impl<C: Serialize>(
        self,
        cursor: C,
        path: &Path,
        native: Option<&Path>,
        validate: impl FnOnce(&Project, &Value, &C) -> io::Result<Vec<u8>>,
        before_publish: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<(PathBuf, ReleaseManifest<C>)> {
        let path = destination(path, native)?;
        let validation = if let Some(checks) = &self.job.checks {
            // Fixed numeric settings serialize to a small bounded bridge, never source strings.
            let input = serde_json::to_value(checks).map_err(invalid_json)?;
            let bytes = validate(&self.board.project, &input, &cursor)?;
            if bytes.len() > VALIDATION_LIMIT {
                return Err(invalid("checked validation artifact exceeds16MiB"));
            }
            Some(bytes)
        } else {
            None
        };
        let package = self.board.render(FabricationOptions { mask_expansion_mm: self.job.mask_expansion_mm })?;
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for (name, data) in package.files {
            add_file(&mut files, &mut total, format!("fabrication/{name}"), data.into_bytes())?;
        }
        add_file(&mut files, &mut total, "job.json".into(), self.job.canonical_bytes()?)?;
        if let Some(bytes) = validation {
            add_file(&mut files, &mut total, "validation/drc.json".into(), bytes)?;
        }
        for (name, kind) in [("assembly/bom.csv", AssemblyCsv::Bom), ("assembly/placement.csv", AssemblyCsv::Placement)] {
            let data = render_csv_limited(self.board.project.board.parts.values(), kind, RELEASE_LIMIT - total)?;
            add_file(&mut files, &mut total, name.into(), data.into_bytes())?;
        }
        let manifest = ReleaseManifest {
            format: "viamind-output-release".into(),
            version: 1,
            project_id: self.board.project.id.to_string(),
            exported_cursor: cursor,
            files: files.iter().map(|(n, b)| (n.clone(), b.len())).collect(),
            exclusions: package
                .manifest
                .exclusions
                .into_iter()
                .chain([
                    "Generic placement records component origins, not manufacturer-validated assembly centroids.".into(),
                    "No sourcing or population metadata, manufacturer presets or vendor certification.".into(),
                ])
                .collect(),
        };
        let bytes = bounded_json(&manifest, RELEASE_LIMIT - total)?;
        add_file(&mut files, &mut total, "release-manifest.json".into(), bytes)?;
        #[cfg(not(target_os = "linux"))]
        return Err(io::Error::new(io::ErrorKind::Unsupported, "atomic output jobs initially support Linux"));
        #[cfg(target_os = "linux")]
        {
            let stage = tempfile::Builder::new().prefix(".viamind-output-job-").tempdir_in(path.parent().expect("canonical parent"))?;
            let dirs = if self.job.checks.is_some() { vec!["assembly", "fabrication", "validation"] } else { vec!["assembly", "fabrication"] };
            for dir in &dirs {
                std::fs::create_dir(stage.path().join(dir))?;
            }
            for (name, bytes) in files {
                let mut file = OpenOptions::new().write(true).create_new(true).open(stage.path().join(name))?;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            for dir in dirs.into_iter().chain([""]) {
                File::open(stage.path().join(dir))?.sync_all()?;
            }
            before_publish()?;
            rustix::fs::renameat_with(rustix::fs::CWD, stage.path(), rustix::fs::CWD, &path, rustix::fs::RenameFlags::NOREPLACE)?;
            Ok((path, manifest))
        }
    }
}
fn add_file(files: &mut BTreeMap<String, Vec<u8>>, total: &mut usize, name: String, data: Vec<u8>) -> io::Result<()> {
    if data.len() > RELEASE_LIMIT.saturating_sub(*total) {
        return Err(invalid("output release exceeds 192 MiB"));
    }
    *total += data.len();
    files.insert(name, data);
    Ok(())
}
struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(invalid("JSON output exceeds byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn bounded_json(value: &impl Serialize, limit: usize) -> io::Result<Vec<u8>> {
    let mut writer = BoundedBytes { bytes: Vec::new(), limit };
    serde_json::to_writer_pretty(&mut writer, value).map_err(invalid_json)?;
    writer.write_all(b"\n")?;
    Ok(writer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_limits_reject_before_append() {
        assert!(bounded_json(&"long string", 4).is_err());
        assert!(crate::assembly::render_csv_limited(std::iter::empty(), AssemblyCsv::Bom, 4).is_err());
        let mut files = BTreeMap::new();
        let mut total = RELEASE_LIMIT;
        assert!(add_file(&mut files, &mut total, "excess".into(), vec![1]).is_err());
        assert!(files.is_empty());
        assert_eq!(total, RELEASE_LIMIT);
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    #[test]
    fn raced_destination_and_injected_post_stage_failure_leave_no_partial_release() {
        let _guard = SERIAL.lock().unwrap();
        let p = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("release");
        let snapshot = OutputJobSnapshot::capture(&p, OutputJob::new("race".into(), 0.)).unwrap();
        let error = snapshot
            .write_with_before_publish(0, &path, None, || {
                std::fs::create_dir(&path)?;
                std::fs::write(path.join("sentinel"), b"winner")
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"winner");
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        let failure = temp.path().join("failed");
        let snapshot = OutputJobSnapshot::capture(&p, OutputJob::new("failure".into(), 0.)).unwrap();
        assert!(
            snapshot.write_with_before_publish(0, &failure, None, || Err(io::Error::other("injected sync/publication boundary failure"))).is_err()
        );
        assert!(!failure.exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    #[test]
    fn one_job_uses_one_permit_and_unrelated_invalid_schematic_is_excluded() {
        let _guard = SERIAL.lock().unwrap();
        let mut p = Project::new("board");
        let mut sheet = viamind_doc::Sheet::new("Unrelated");
        sheet.name = "x".repeat(4097);
        p.schematic.sheets.insert(sheet.id, sheet);
        let job = OutputJobSnapshot::capture(&p, OutputJob::new("capture".into(), 0.)).unwrap();
        assert!(job.board.project.schematic.sheets.is_empty());
        let other = crate::AssemblySnapshot::capture(&p).unwrap();
        assert!(crate::AssemblySnapshot::capture(&p).is_err());
        drop(other);
        drop(job);
        assert!(crate::AssemblySnapshot::capture(&p).is_ok());
    }
}
