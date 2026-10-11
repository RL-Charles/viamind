use serde_json::json;
use std::{fs, sync::Mutex};
use viamind_fab::{AssemblyCsv, AssemblySnapshot, FabricationSnapshot, OutputJob, OutputJobSnapshot};
static SERIAL: Mutex<()> = Mutex::new(());
fn project() -> viamind_doc::Project {
    viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/calibration.viamind.json")).unwrap()
}
fn job() -> OutputJob {
    let mut job = OutputJob::new("Calibrated".into(), 0.);
    job.version = 2;
    job.include_calibrated_placement = Some(true);
    job
}
#[test]
fn explicit_version_two_is_strict_and_legacy_canonical_bytes_are_unchanged() {
    let _serial = SERIAL.lock().unwrap();
    let original = OutputJob::new("Legacy".into(), 0.);
    assert_eq!(original.canonical_bytes().unwrap(), b"{\n  \"format\": \"viamind-output-job\",\n  \"version\": 1,\n  \"name\": \"Legacy\",\n  \"profile\": \"generic_outer_2layer\",\n  \"mask_expansion_mm\": 0.0\n}\n");
    let accepted = job().canonical_bytes().unwrap();
    assert_eq!(OutputJob::parse(&accepted).unwrap(), job());
    for (version, field) in [
        (1, Some(json!(true))),
        (1, Some(json!(false))),
        (1, Some(json!(null))),
        (2, None),
        (2, Some(json!(false))),
        (2, Some(json!(null))),
        (2, Some(json!(1))),
        (3, Some(json!(true))),
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["version"] = json!(version);
        if let Some(field) = field {
            value["include_calibrated_placement"] = field;
        }
        assert!(OutputJob::parse(&serde_json::to_vec(&value).unwrap()).is_err(), "{value}");
    }
    let duplicate = String::from_utf8(accepted)
        .unwrap()
        .replace("\"include_calibrated_placement\": true", "\"include_calibrated_placement\": true,\"include_calibrated_placement\": true");
    assert!(OutputJob::parse(duplicate.as_bytes()).is_err());
    let mut unknown = serde_json::to_value(job()).unwrap();
    unknown["outputs"] = json!(["anything"]);
    assert!(OutputJob::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
}
#[test]
fn companion_has_standalone_bytes_and_stable_population_without_changing_legacy_payloads() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    source.board.parts.values_mut().find(|p| p.designator == "U_TOP").unwrap().rotation_mdeg = 31_234;
    source.board.parts.values_mut().find(|p| p.designator == "U_BOTTOM").unwrap().assembly.as_mut().unwrap().populate = false;
    let original = source.clone();
    let expected = viamind_fab::csv(&source, AssemblyCsv::AssemblyPlacement).unwrap();
    let old_job = OutputJob::new("Legacy".into(), 0.);
    let temp = tempfile::tempdir().unwrap();
    let (_, old) = OutputJobSnapshot::capture(&source, old_job).unwrap().write(42, &temp.path().join("legacy"), None).unwrap();
    let captured = OutputJobSnapshot::capture(&source, job()).unwrap();
    source.board.parts.clear();
    source.board.net_rules.clear();
    let (path, manifest) = captured.write(42, &temp.path().join("calibrated"), None).unwrap();
    assert_eq!(manifest.version, 2);
    assert_eq!(manifest.files.len() + 1, 12);
    let summary = manifest.calibrated_placement.as_ref().unwrap();
    assert_eq!(summary.component_count, 1);
    assert_eq!(summary.coordinate_convention, "global_x_right_y_up_ccw_common_top_view");
    assert_eq!(fs::read_to_string(path.join(&summary.file)).unwrap(), expected);
    for name in old.files.keys().filter(|name| name.as_str() != "job.json") {
        assert_eq!(fs::read(path.join(name)).unwrap(), fs::read(temp.path().join("legacy").join(name)).unwrap(), "{name}");
    }
    assert!(manifest.exclusions.iter().any(|s| s.contains("not a matched populated assembly set")));
    assert!(!manifest.exclusions.iter().any(|s| s.starts_with("No sourcing or population metadata")));
    assert_eq!(original.board.parts.len(), 2);
    let bytes = fs::read(path.join("release-manifest.json")).unwrap();
    assert!(OutputJobSnapshot::capture(&original, job()).unwrap().write(42, &path, None).is_err());
    assert_eq!(fs::read(path.join("release-manifest.json")).unwrap(), bytes);
}
#[test]
fn purpose_capture_excludes_unused_source_strings_and_sheets_and_admission_is_one_slot() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    for part in source.board.parts.values_mut() {
        let metadata = part.assembly.as_mut().unwrap();
        metadata.manufacturer = Some("x".repeat(3 * 1024 * 1024));
        metadata.supplier = Some("invalid\nsource".into());
    }
    let mut sheet = viamind_doc::Sheet::new("ignored");
    sheet.name = "x".repeat(4097);
    source.schematic.sheets.insert(sheet.id, sheet);
    let captured = OutputJobSnapshot::capture(&source, job()).unwrap();
    let second = FabricationSnapshot::capture(&source).unwrap();
    assert!(AssemblySnapshot::capture(&source).is_err());
    drop(second);
    let temp = tempfile::tempdir().unwrap();
    let (path, _) = captured.write(0, &temp.path().join("release"), None).unwrap();
    assert!(fs::read_to_string(path.join("assembly/calibrated-placement.csv")).unwrap().contains("U_BOTTOM"));
    assert!(source.board.parts.values().all(|p| p.assembly.as_ref().unwrap().manufacturer.as_ref().unwrap().len() == 3 * 1024 * 1024));
    assert!(AssemblySnapshot::capture(&source).is_ok());
}
#[test]
fn incomplete_calibration_collision_and_empty_population_have_atomic_results() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    source.board.parts.values_mut().next().unwrap().assembly.as_mut().unwrap().placement = None;
    assert!(OutputJobSnapshot::capture(&source, job()).is_err());
    // Default legacy jobs continue to ignore unused incomplete calibration.
    assert!(OutputJobSnapshot::capture(&source, OutputJob::new("Legacy".into(), 0.)).is_ok());
    source = project();
    for part in source.board.parts.values_mut() {
        part.designator = "COLLIDE".into();
    }
    assert!(OutputJobSnapshot::capture(&source, job()).is_err());
    for part in source.board.parts.values_mut() {
        let metadata = part.assembly.as_mut().unwrap();
        metadata.populate = false;
        metadata.placement = None;
        metadata.mounting = None;
    }
    let temp = tempfile::tempdir().unwrap();
    let (path, manifest) = OutputJobSnapshot::capture(&source, job()).unwrap().write(0, &temp.path().join("empty"), None).unwrap();
    assert_eq!(manifest.calibrated_placement.unwrap().component_count, 0);
    assert_eq!(fs::read_to_string(path.join("assembly/calibrated-placement.csv")).unwrap().lines().count(), 1);
    let native = temp.path().join("native");
    fs::write(&native, b"native").unwrap();
    assert!(OutputJobSnapshot::capture(&source, job()).unwrap().write(0, &native, Some(&native)).is_err());
    assert_eq!(fs::read(&native).unwrap(), b"native");
}
