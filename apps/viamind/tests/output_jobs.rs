use std::{fs, process::Command};
#[test]
fn real_headless_saved_job_release_and_separate_mode_validation() {
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    let job_path = temp.path().join("job.json");
    let output = temp.path().join("release");
    fs::write(&native, include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let original = fs::read(&native).unwrap();
    let job = viamind_fab::OutputJob::new("CLI release".into(), 0.050001);
    viamind_fab::save_output_job(&job, &job_path, None).unwrap();
    let args = ["--run-output-job".as_ref(), job_path.as_os_str(), "--open".as_ref(), native.as_os_str(), "--output".as_ref(), output.as_os_str()];
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_viamind")).args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").output().unwrap()
    };
    let result = run(&args);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(output.join("release-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["files"].as_object().unwrap().len(), 10);
    assert!(manifest["exported_cursor"]["stream_id"].is_string());
    let project = viamind_doc::file::load(&native).unwrap();
    for (name, kind) in [("bom.csv", viamind_fab::AssemblyCsv::Bom), ("placement.csv", viamind_fab::AssemblyCsv::Placement)] {
        assert_eq!(fs::read(output.join("assembly").join(name)).unwrap(), viamind_fab::csv(&project, kind).unwrap().into_bytes());
    }
    let bytes = fs::read(output.join("release-manifest.json")).unwrap();
    assert!(!run(&args).status.success());
    assert_eq!(fs::read(output.join("release-manifest.json")).unwrap(), bytes);
    for extra in ["--headless", "--overwrite", "--export-gerber", "--export-svg", "--export-schematic-png", "--no-mcp"] {
        let mut bad = args.to_vec();
        bad.push(extra.as_ref());
        assert!(!run(&bad).status.success());
    }
    assert!(!run(&["--run-output-job".as_ref(), job_path.as_os_str()]).status.success());
    assert_eq!(fs::read(native).unwrap(), original);
}

#[test]
fn checked_headless_job_reports_same_snapshot_and_failure_publishes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    fs::write(&native, include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let original = fs::read(&native).unwrap();
    let mut job = viamind_fab::OutputJob::new("Checked CLI".into(), 0.);
    job.profile = viamind_fab::CHECKED_PROFILE.into();
    job.checks = Some(viamind_fab::OutputJobChecks {
        copper_clearance_mm: 0.,
        edge_clearance_mm: 0.,
        minimum_via_drill_mm: 0.,
        minimum_annular_width_mm: 0.,
        hole_clearance_mm: 0.,
        hole_to_copper_clearance_mm: 0.,
        max_violations: 1000,
    });
    let settings = temp.path().join("checked.json");
    viamind_fab::save_output_job(&job, &settings, None).unwrap();
    let output = temp.path().join("checked");
    let run = |settings: &std::path::Path, output: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_viamind"))
            .args(["--run-output-job".as_ref(), settings.as_os_str(), "--open".as_ref(), native.as_os_str(), "--output".as_ref(), output.as_os_str()])
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap()
    };
    let result = run(&settings, &output);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let validation: serde_json::Value = serde_json::from_slice(&fs::read(output.join("validation/drc.json")).unwrap()).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(output.join("release-manifest.json")).unwrap()).unwrap();
    assert_eq!(validation["exported_cursor"], manifest["exported_cursor"]);
    assert_eq!(validation["project_id"], manifest["project_id"]);
    assert_eq!(manifest["files"].as_object().unwrap().len(), 11);
    assert_eq!(validation["report"]["violations"], serde_json::json!([]));
    job.checks.as_mut().unwrap().edge_clearance_mm = 100.;
    let strict = temp.path().join("strict.json");
    viamind_fab::save_output_job(&job, &strict, None).unwrap();
    let refused = temp.path().join("refused");
    let result = run(&strict, &refused);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("violations"));
    assert!(!refused.exists());
    assert_eq!(fs::read(&native).unwrap(), original);
}
