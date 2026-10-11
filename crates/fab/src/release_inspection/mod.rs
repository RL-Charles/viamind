//! Detached consistency facts, never fabrication authorization or content authentication.
mod csv;
#[cfg(target_os = "linux")]
mod reader;
use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, Visitor},
};
use std::{collections::BTreeMap, fmt, io, path::PathBuf};
use viamind_doc::ObjectId;

const JSON_LIMIT: usize = 64 * 1024;
const RELEASE_LIMIT: usize = 192 * 1024 * 1024;
const CSV_LIMIT: usize = 32 * 1024 * 1024;
const FAB_LIMIT: usize = 128 * 1024 * 1024;
const BASE_FILES: [&str; 10] = [
    "job.json",
    "assembly/bom.csv",
    "assembly/placement.csv",
    "fabrication/top-copper.gbr",
    "fabrication/bottom-copper.gbr",
    "fabrication/top-mask.gbr",
    "fabrication/bottom-mask.gbr",
    "fabrication/profile.gbr",
    "fabrication/plated-through.xnc",
    "fabrication/manifest.json",
];
const CALIBRATED: &str = "assembly/calibrated-placement.csv";
const POPULATED: &str = "assembly/populated-bom.csv";
const VALIDATION: &str = "validation/drc.json";
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectOutputRelease {
    pub path: PathBuf,
}
pub fn release_inspection_schema() -> serde_json::Value {
    serde_json::json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{"path":{"type":"string","minLength":1,"maxLength":4096}}})
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCursor {
    pub stream_id: ObjectId,
    pub sequence: u64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MatchedAssembly {
    pub component_count: usize,
    pub component_ids_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReleaseFileFact {
    pub path: String,
    pub declared_bytes: usize,
    pub observed_bytes: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReleaseInspection {
    pub path: PathBuf,
    pub format: String,
    pub version: u32,
    pub profile: String,
    pub project_id: ObjectId,
    pub exported_cursor: ReleaseCursor,
    pub file_count: usize,
    pub observed_bytes: usize,
    pub files: Vec<ReleaseFileFact>,
    pub calibrated_component_count: Option<usize>,
    pub matched_assembly: Option<MatchedAssembly>,
    pub release_exclusions: Vec<String>,
    pub checks_performed: Vec<String>,
    pub checks_not_performed: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Calibrated {
    file: String,
    component_count: usize,
    coordinate_convention: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Populated {
    bom_file: String,
    placement_file: String,
    component_count: usize,
    component_ids_sha256: String,
}
fn nonnull<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    project_id: ObjectId,
    exported_cursor: ReleaseCursor,
    #[serde(deserialize_with = "file_map")]
    files: BTreeMap<String, usize>,
    exclusions: Vec<String>,
    #[serde(default, deserialize_with = "nonnull")]
    calibrated_placement: Option<Calibrated>,
    #[serde(default, deserialize_with = "nonnull")]
    populated_assembly: Option<Populated>,
}
fn file_map<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, usize>, D::Error> {
    struct Files;
    impl<'de> Visitor<'de> for Files {
        type Value = BTreeMap<String, usize>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most13 unique fixed release file entries")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut files = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if files.len() >= 13 || key.len() > 64 || files.contains_key(&key) {
                    return Err(de::Error::custom("duplicate/excess/oversized release file key"));
                }
                let size = map.next_value::<usize>()?;
                if size > RELEASE_LIMIT {
                    return Err(de::Error::custom("declared file exceeds release limit"));
                }
                files.insert(key, size);
            }
            Ok(files)
        }
    }
    d.deserialize_map(Files)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FabManifest {
    project_id: ObjectId,
    units: String,
    coordinate_system: String,
    mask_expansion_nm: i64,
    smd_pad_count: usize,
    track_count: usize,
    through_via_count: usize,
    cutout_count: usize,
    files: Vec<String>,
    exclusions: Vec<String>,
}
fn json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> io::Result<T> {
    if bytes.len() > JSON_LIMIT {
        return Err(invalid("inspection JSON exceeds64KiB"));
    }
    serde_json::from_slice(bytes).map_err(|e| invalid(format!("invalid strict release JSON: {e}")))
}
fn exclusions(rows: &[String]) -> io::Result<()> {
    if rows.len() > 16 || rows.iter().any(|row| row.len() > 4096) {
        return Err(invalid("exclusions exceed16 rows or4096 bytes/row"));
    }
    Ok(())
}
impl Manifest {
    fn validate(&self, job: &crate::OutputJob) -> io::Result<()> {
        job.validate()?;
        if self.format != "viamind-output-release" || self.version != job.version {
            return Err(invalid("release format/version differs from supported job"));
        }
        let mut expected = BASE_FILES.to_vec();
        if job.version >= 2 {
            expected.push(CALIBRATED);
        }
        if job.version == 3 {
            expected.push(POPULATED);
        }
        if job.profile == crate::CHECKED_PROFILE {
            expected.push(VALIDATION);
        }
        if expected.len() != self.files.len() || expected.iter().any(|name| !self.files.contains_key(*name)) {
            return Err(invalid("release has missing or unknown fixed file paths"));
        }
        if self.files.values().try_fold(0usize, |sum, n| sum.checked_add(*n)).is_none_or(|sum| sum > RELEASE_LIMIT) {
            return Err(invalid("declared release exceeds192MiB"));
        }
        exclusions(&self.exclusions)?;
        match (&self.calibrated_placement, job.version) {
            (None, 1) => {}
            (Some(cal), 2 | 3)
                if cal.file == CALIBRATED
                    && cal.component_count <= 100_000
                    && cal.coordinate_convention == "global_x_right_y_up_ccw_common_top_view" => {}
            _ => return Err(invalid("calibrated summary contradicts release version/count/convention")),
        }
        match (&self.populated_assembly, job.version) {
            (None, 1 | 2) => {}
            (Some(pop), 3)
                if pop.bom_file == POPULATED
                    && pop.placement_file == CALIBRATED
                    && self.calibrated_placement.as_ref().is_some_and(|c| c.component_count == pop.component_count)
                    && pop.component_ids_sha256.len() == 64
                    && pop.component_ids_sha256.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')) => {}
            _ => return Err(invalid("populated summary contradicts release version/paths/count/digest")),
        }
        Ok(())
    }
}
fn validate_fabrication(fab: &FabManifest, manifest: &Manifest, job: &crate::OutputJob) -> io::Result<()> {
    let mut expected = BASE_FILES.iter().filter_map(|p| p.strip_prefix("fabrication/")).collect::<Vec<_>>();
    expected.sort_unstable();
    let mut actual = fab.files.iter().map(String::as_str).collect::<Vec<_>>();
    actual.sort_unstable();
    if fab.project_id != manifest.project_id
        || actual != expected
        || fab.units != "mm (Gerber 5.6, 1 nm; XNC explicit decimals)"
        || fab.coordinate_system != "Model origin; X right; Y up (-model Y); top view for every layer; bottom files are not mirrored"
        || fab.mask_expansion_nm != (job.mask_expansion_mm * 1_000_000.).round() as i64
        || fab.smd_pad_count > 200_000
        || fab.track_count > 200_000
        || fab.through_via_count > 100_000
        || fab.cutout_count > 200_000
    {
        return Err(invalid("fabrication manifest project/files/coordinate/mask/count facts differ from supported release"));
    }
    exclusions(&fab.exclusions)
}
pub fn inspect_output_release(input: &InspectOutputRelease) -> io::Result<ReleaseInspection> {
    #[cfg(target_os = "linux")]
    {
        reader::inspect(input)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = input;
        Err(io::Error::new(io::ErrorKind::Unsupported, "release inspection initially supports Linux"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_map_rejects_duplicates_before_collapsing() {
        #[derive(Deserialize)]
        struct Test {
            #[serde(deserialize_with = "file_map")]
            files: BTreeMap<String, usize>,
        }
        assert!(json::<Test>(br#"{"files":{"job.json":1,"job.json":2}}"#).is_err());
        assert_eq!(json::<Test>(br#"{"files":{"job.json":1}}"#).unwrap().files.len(), 1);
    }
}
