use std::{fs, path::Path};
use viamind_fab::{AssemblyCsv, FabricationOptions, OutputJob, OutputJobSnapshot, load_output_job, save_output_job};
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn project() -> viamind_doc::Project {
    viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
}
fn job() -> OutputJob {
    OutputJob::new("Manufacturing λ".into(), 0.050001)
}
#[test]
fn strict_descriptor_and_bounded_regular_file_loading() {
    let _serial = SERIAL.lock().unwrap();
    let valid = job().canonical_bytes().unwrap();
    assert_eq!(OutputJob::parse(&valid).unwrap(), job());
    for bad in [
        r#"{"format":"viamind-output-job","version":1,"name":"a","name":"b","profile":"generic_outer_2layer"}"#,
        r#"{"format":"viamind-output-job","version":2,"name":"a","profile":"generic_outer_2layer"}"#,
        r#"{"format":"viamind-output-job","version":1,"name":"a","profile":"generic_outer_2layer","script":"bad"}"#,
        r#"{"format":"viamind-output-job","version":1,"name":"a","profile":"generic_outer_2layer","mask_expansion_mm":1e999}"#,
        r#"{"format":"viamind-output-job","version":1,"name":"a","profile":"generic_outer_2layer","mask_expansion_mm":-1}"#,
        r#"{"format":"viamind-output-job","version":1,"name":" ","profile":"generic_outer_2layer"}"#,
        r#"{"format":"viamind-output-job","version":1,"name":"a\nb","profile":"generic_outer_2layer"}"#,
        r#"{"version":1,"name":"a","profile":"generic_outer_2layer"}"#,
    ] {
        assert!(OutputJob::parse(bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(OutputJob::parse(&vec![b' '; 65537]).is_err());
    assert!(OutputJob::parse(&[255]).is_err());
    assert!(OutputJob::parse(format!("{}{}", "[".repeat(200), "]".repeat(200)).as_bytes()).is_err());
    assert!(OutputJob::new("λ".repeat(129), 0.).validate().is_err());
    assert!(OutputJob::new("name".into(), f64::NAN).canonical_bytes().is_err());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("job.json");
    save_output_job(&job(), &path, None).unwrap();
    assert_eq!(load_output_job(&path).unwrap(), job());
    assert!(save_output_job(&job(), &path, None).is_err());
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(load_output_job(temp.path()).is_err());
    #[cfg(target_os = "linux")]
    {
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(load_output_job(&link).is_err());
        let fifo = temp.path().join("fifo");
        rustix::fs::mknodat(rustix::fs::CWD, &fifo, rustix::fs::FileType::Fifo, rustix::fs::Mode::RUSR, 0).unwrap();
        assert!(load_output_job(&fifo).is_err());
        assert!(load_output_job(Path::new("/dev/null")).is_err());
    }
}
#[test]
fn release_is_coherent_byte_identical_and_never_clobbers() {
    let _serial = SERIAL.lock().unwrap();
    let mut project = project();
    let original = project.clone();
    let snapshot = OutputJobSnapshot::capture(&project, job()).unwrap();
    project.name = "later edit".into();
    project.board.parts.clear();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("release");
    let (path, manifest) = snapshot.write(serde_json::json!({"stream_id":"test","sequence":42}), &output, None).unwrap();
    assert_eq!(path, output);
    assert_eq!(manifest.files.len(), 10);
    assert_eq!(manifest.project_id, original.id.to_string());
    assert_eq!(manifest.exported_cursor["sequence"], 42);
    assert_eq!(fs::read(path.join("job.json")).unwrap(), job().canonical_bytes().unwrap());
    for (name, kind) in [("bom.csv", AssemblyCsv::Bom), ("placement.csv", AssemblyCsv::Placement)] {
        assert_eq!(fs::read(path.join("assembly").join(name)).unwrap(), viamind_fab::csv(&original, kind).unwrap().into_bytes());
    }
    let fab = viamind_fab::fabrication(
        &original,
        FabricationOptions { mask_expansion_mm: job().mask_expansion_mm, include_silkscreen: false, include_paste: false },
    )
    .unwrap();
    for (name, data) in fab.files {
        assert_eq!(fs::read(path.join("fabrication").join(name)).unwrap(), data.into_bytes());
    }
    for (name, size) in &manifest.files {
        assert_eq!(fs::metadata(path.join(name)).unwrap().len(), *size as u64);
    }
    let contents = fs::read(path.join("release-manifest.json")).unwrap();
    assert!(OutputJobSnapshot::capture(&original, job()).unwrap().write(0, &output, None).is_err());
    assert_eq!(fs::read(path.join("release-manifest.json")).unwrap(), contents);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    let native = temp.path().join("native.viamind.json");
    fs::write(&native, b"native").unwrap();
    assert!(OutputJobSnapshot::capture(&original, job()).unwrap().write(0, &native, Some(&native)).is_err());
    assert_eq!(fs::read(&native).unwrap(), b"native");
    assert!(save_output_job(&job(), &native, Some(&native)).is_err());
    #[cfg(target_os = "linux")]
    {
        let link = temp.path().join("alias");
        std::os::unix::fs::symlink(&native, &link).unwrap();
        assert!(OutputJobSnapshot::capture(&original, job()).unwrap().write(0, &link, None).is_err());
    }
}
#[test]
fn release_failure_cleans_staging_and_rejects_board_budget_before_copy() {
    let _serial = SERIAL.lock().unwrap();
    let mut p = project();
    let before = p.clone();
    let temp = tempfile::tempdir().unwrap();
    // Fabrication rejects a legacy preview before publishing any output.
    p.board.parts.values_mut().next().unwrap().pads = None;
    let output = temp.path().join("failed");
    assert!(OutputJobSnapshot::capture(&p, job()).unwrap().write(0, &output, None).is_err());
    assert!(!output.exists());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    p = before;
    p.name = "x".repeat(4097);
    assert!(OutputJobSnapshot::capture(&p, job()).is_err());
    assert_eq!(p.name.len(), 4097);
}

#[test]
fn job_component_budget_precedes_board_capture() {
    let _serial = SERIAL.lock().unwrap();
    let mut p = project();
    let mut template = p.board.parts.values().next().unwrap().clone();
    template.pads = None;
    p.board.parts.clear();
    for _ in 0..100_001 {
        let mut part = template.clone();
        part.id = viamind_doc::ObjectId::new();
        p.board.parts.insert(part.id, part);
    }
    let error = OutputJobSnapshot::capture(&p, job()).err().unwrap();
    assert!(error.to_string().contains("100000 components"));
    assert_eq!(p.board.parts.len(), 100_001);
}

fn checked_job() -> OutputJob {
    let mut job = job();
    job.profile = viamind_fab::CHECKED_PROFILE.into();
    job.checks = Some(viamind_fab::OutputJobChecks {
        copper_clearance_mm: 0.2,
        edge_clearance_mm: 0.25,
        minimum_via_drill_mm: 0.2,
        minimum_annular_width_mm: 0.1,
        hole_clearance_mm: 0.25,
        hole_to_copper_clearance_mm: 0.2,
        max_violations: 1000,
    });
    job
}
#[test]
fn strict_checked_descriptor_requires_every_numeric_baseline_and_preserves_generic_bytes() {
    let original = job();
    assert!(!String::from_utf8(original.canonical_bytes().unwrap()).unwrap().contains("checks"));
    let checked = checked_job();
    assert_eq!(OutputJob::parse(&checked.canonical_bytes().unwrap()).unwrap(), checked);
    let value = serde_json::to_value(&checked).unwrap();
    for field in [
        "copper_clearance_mm",
        "edge_clearance_mm",
        "minimum_via_drill_mm",
        "minimum_annular_width_mm",
        "hole_clearance_mm",
        "hole_to_copper_clearance_mm",
        "max_violations",
    ] {
        let mut missing = value.clone();
        missing["checks"].as_object_mut().unwrap().remove(field);
        assert!(OutputJob::parse(&serde_json::to_vec(&missing).unwrap()).is_err(), "{field}");
    }
    for (field, bad) in [("profile", serde_json::json!("generic_outer_2layer")), ("checks", serde_json::Value::Null)] {
        let mut invalid = value.clone();
        invalid[field] = bad;
        assert!(OutputJob::parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("checks");
    assert!(OutputJob::parse(&serde_json::to_vec(&missing).unwrap()).is_err());
    for bad in [serde_json::json!(-0.1), serde_json::json!(100.1), serde_json::json!("0.2")] {
        let mut invalid = value.clone();
        invalid["checks"]["copper_clearance_mm"] = bad;
        assert!(OutputJob::parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut invalid = value.clone();
    invalid["checks"]["pass"] = serde_json::json!(true);
    assert!(OutputJob::parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
    let mut null_generic = serde_json::to_value(original).unwrap();
    null_generic["checks"] = serde_json::Value::Null;
    assert!(OutputJob::parse(&serde_json::to_vec(&null_generic).unwrap()).is_err());
}
#[test]
fn checked_snapshot_mandates_validation_before_output_and_keeps_one_coherent_board() {
    let _serial = SERIAL.lock().unwrap();
    let mut project = project();
    let original = project.clone();
    let temp = tempfile::tempdir().unwrap();
    let job = checked_job();
    assert!(OutputJobSnapshot::capture(&project, job.clone()).unwrap().write(42, &temp.path().join("bypass"), None).is_err());
    let snapshot = OutputJobSnapshot::capture(&project, job.clone()).unwrap();
    project.board.parts.clear();
    let (path, manifest) = snapshot
        .write_with_validation(42, &temp.path().join("checked"), None, |captured, checks, cursor| {
            assert_eq!(captured.board, original.board.clone_without_assembly());
            assert_eq!(*cursor, 42);
            assert_eq!(checks["max_violations"], 1000);
            Ok(b"{\"trusted_test_callback\":true}\n".to_vec())
        })
        .unwrap();
    assert_eq!(manifest.files.len(), 11);
    assert_eq!(fs::read(path.join("validation/drc.json")).unwrap(), b"{\"trusted_test_callback\":true}\n");
    for (name, bytes) in viamind_fab::fabrication(
        &original,
        FabricationOptions { mask_expansion_mm: job.mask_expansion_mm, include_silkscreen: false, include_paste: false },
    )
    .unwrap()
    .files
    {
        assert_eq!(fs::read(path.join("fabrication").join(name)).unwrap(), bytes.into_bytes());
    }
    let failed = temp.path().join("failed");
    let error = OutputJobSnapshot::capture(&original, job.clone())
        .unwrap()
        .write_with_validation(0, &failed, None, |_, _, _| Err(std::io::Error::other("trusted DRC refusal")))
        .unwrap_err();
    assert!(error.to_string().contains("DRC refusal"));
    assert!(!failed.exists());
    assert!(
        OutputJobSnapshot::capture(&original, job)
            .unwrap()
            .write_with_validation(0, &failed, None, |_, _, _| Ok(vec![0; 16 * 1024 * 1024 + 1]))
            .is_err()
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    OutputJobSnapshot::capture(&original, original_job())
        .unwrap()
        .write_with_validation(0, &temp.path().join("generic"), None, |_, _, _| panic!("generic must not invoke validator"))
        .unwrap();
}
fn original_job() -> OutputJob {
    job()
}
