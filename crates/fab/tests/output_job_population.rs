use serde_json::json;
use std::{fs, sync::Mutex};
use viamind_fab::{AssemblyCsv, AssemblySnapshot, FabricationSnapshot, OutputJob, OutputJobSnapshot};
static SERIAL: Mutex<()> = Mutex::new(());
fn project() -> viamind_doc::Project {
    viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/output-job-calibration.viamind.json")).unwrap()
}
fn job(version: u32) -> OutputJob {
    let mut job = OutputJob::new("Matched".into(), 0.);
    job.version = version;
    job.include_calibrated_placement = (version >= 2).then_some(true);
    job.include_populated_bom = (version == 3).then_some(true);
    job
}
#[test]
fn explicit_matched_version_three_does_not_reinterpret_legacy_settings() {
    let _serial = SERIAL.lock().unwrap();
    assert_eq!(job(2).canonical_bytes().unwrap(), b"{\n  \"format\": \"viamind-output-job\",\n  \"version\": 2,\n  \"name\": \"Matched\",\n  \"profile\": \"generic_outer_2layer\",\n  \"mask_expansion_mm\": 0.0,\n  \"include_calibrated_placement\": true\n}\n");
    assert_eq!(OutputJob::parse(&job(3).canonical_bytes().unwrap()).unwrap(), job(3));
    for version in 1..=3 {
        for field in [None, Some(json!(true)), Some(json!(false)), Some(json!(null)), Some(json!(1))] {
            let mut value = serde_json::to_value(job(version)).unwrap();
            value.as_object_mut().unwrap().remove("include_populated_bom");
            if let Some(field) = &field {
                value["include_populated_bom"] = field.clone();
            }
            let accepted = field.as_ref().is_none_or(|f| f == &json!(true))
                && ((version < 3 && field.is_none()) || (version == 3 && field == Some(json!(true))));
            assert_eq!(OutputJob::parse(&serde_json::to_vec(&value).unwrap()).is_ok(), accepted, "{value}");
        }
    }
    let duplicate = String::from_utf8(job(3).canonical_bytes().unwrap())
        .unwrap()
        .replace("\"include_populated_bom\": true", "\"include_populated_bom\": true,\"include_populated_bom\": true");
    assert!(OutputJob::parse(duplicate.as_bytes()).is_err());
    let mut value = serde_json::to_value(job(3)).unwrap();
    value.as_object_mut().unwrap().remove("include_calibrated_placement");
    assert!(OutputJob::parse(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn matched_bom_has_exact_explicit_fields_digest_and_unchanged_calibrated_inventory_bytes() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    for part in source.board.parts.values_mut() {
        let metadata = part.assembly.as_mut().unwrap();
        if part.side == viamind_doc::Side::Bottom {
            metadata.populate = false;
            metadata.manufacturer = Some("x".repeat(3 * 1024 * 1024));
            metadata.placement = None;
        } else {
            part.value = "µ, \"parts\"\nline".into();
            metadata.mounting = Some(viamind_doc::MountingTechnology::ThroughHole);
            metadata.manufacturer = Some("Explicit, \"maker\"".into());
            metadata.mpn = Some("MPN-λ".into());
            metadata.supplier = None;
            metadata.supplier_sku = Some("SKU-42".into());
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let calibrated = viamind_fab::csv(&source, AssemblyCsv::AssemblyPlacement).unwrap();
    let (old_path, old) = OutputJobSnapshot::capture(&source, job(2)).unwrap().write(7, &temp.path().join("v2"), None).unwrap();
    let capture = OutputJobSnapshot::capture(&source, job(3)).unwrap();
    source.board.parts.clear();
    source.board.net_rules.clear();
    let (path, manifest) = capture.write(7, &temp.path().join("v3"), None).unwrap();
    let summary = manifest.populated_assembly.as_ref().unwrap();
    assert_eq!(manifest.version, 3);
    assert_eq!(manifest.files.len() + 1, 13);
    assert_eq!(summary.component_count, 1);
    // Independently generated with Python hashlib over the declared domain and UUID.bytes.
    assert_eq!(summary.component_ids_sha256, "d8f858ebd845a6605cac3a5e16817876f2d41f2132c2de8f7d12a2a36c0d7862");
    assert_eq!(fs::read_to_string(path.join(&summary.placement_file)).unwrap(), calibrated);
    assert_eq!(
        fs::read_to_string(path.join(&summary.bom_file)).unwrap(),
        "Designator,Quantity,Value,Footprint,Mounting,Manufacturer,MPN,Supplier,Supplier SKU,Component ID\r\nU_TOP,1,\"µ, \"\"parts\"\"\nline\",EXPLICIT_FIXTURE,through_hole,\"Explicit, \"\"maker\"\"\",MPN-λ,,SKU-42,00000000-0000-0000-0000-000000000005\r\n"
    );
    for name in old.files.keys().filter(|name| name.as_str() != "job.json") {
        assert_eq!(fs::read(old_path.join(name)).unwrap(), fs::read(path.join(name)).unwrap(), "{name}");
    }
    assert!(serde_json::to_value(old).unwrap().get("populated_assembly").is_none());
    assert!(manifest.exclusions.iter().any(|s| s.contains("same explicitly populated")));
    assert!(!manifest.exclusions.iter().any(|s| s.contains("not a matched populated")));
}
#[test]
fn included_source_preflight_precedes_calibration_and_one_capture_retains_one_permit() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    let id = *source.board.parts.keys().next().unwrap();
    source.board.parts.get_mut(&id).unwrap().assembly.as_mut().unwrap().manufacturer = Some("x".repeat(513));
    source.board.parts.get_mut(&id).unwrap().assembly.as_mut().unwrap().placement = None;
    let error = OutputJobSnapshot::capture(&source, job(3)).err().unwrap();
    assert!(error.to_string().contains("1–512"), "{error}");
    source = project();
    let captured = OutputJobSnapshot::capture(&source, job(3)).unwrap();
    let other = FabricationSnapshot::capture(&source).unwrap();
    assert_eq!(AssemblySnapshot::capture(&source).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    drop(other);
    drop(captured);
    assert!(AssemblySnapshot::capture(&source).is_ok());
    for part in source.board.parts.values_mut() {
        part.assembly.as_mut().unwrap().manufacturer = Some("x".repeat(3 * 1024 * 1024));
    }
    assert!(OutputJobSnapshot::capture(&source, job(2)).is_ok());
    assert!(OutputJobSnapshot::capture(&source, job(3)).is_err());
}
#[test]
fn empty_population_is_headers_only_and_invalid_included_id_mapping_rejects_before_copy() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    for part in source.board.parts.values_mut() {
        let metadata = part.assembly.as_mut().unwrap();
        metadata.populate = false;
        metadata.placement = None;
        metadata.mounting = None;
        metadata.mpn = Some("invalid\nunused".into());
    }
    let temp = tempfile::tempdir().unwrap();
    let (path, manifest) = OutputJobSnapshot::capture(&source, job(3)).unwrap().write(0, &temp.path().join("empty"), None).unwrap();
    assert_eq!(manifest.populated_assembly.unwrap().component_count, 0);
    assert_eq!(fs::read_to_string(path.join("assembly/populated-bom.csv")).unwrap().lines().count(), 1);
    let native = temp.path().join("native");
    fs::write(&native, b"native unchanged").unwrap();
    assert!(OutputJobSnapshot::capture(&source, job(3)).unwrap().write(0, &native, Some(&native)).is_err());
    assert_eq!(fs::read(native).unwrap(), b"native unchanged");
    source = project();
    source.board.parts.values_mut().next().unwrap().id = viamind_doc::ObjectId::new();
    assert!(OutputJobSnapshot::capture(&source, job(3)).err().unwrap().to_string().contains("map key"));
}
#[test]
fn aggregate_included_sourcing_budget_refuses_before_copy_without_restricting_v2() {
    let _serial = SERIAL.lock().unwrap();
    let mut source = project();
    let mut template = source.board.parts.values().next().unwrap().clone();
    template.assembly.as_mut().unwrap().manufacturer = Some("x".repeat(512));
    source.board.parts.clear();
    for index in 0..4097 {
        let mut part = template.clone();
        part.id = viamind_doc::ObjectId::new();
        part.designator = format!("R{index}");
        source.board.parts.insert(part.id, part);
    }
    let error = OutputJobSnapshot::capture(&source, job(3)).err().unwrap();
    assert!(error.to_string().contains("2 MiB aggregate"), "{error}");
    assert_eq!(source.board.parts.len(), 4097);
    assert!(OutputJobSnapshot::capture(&source, job(2)).is_ok());
}
