use std::{fs, process::Command};
#[test]
fn headless_cli_optional_companion_matches_standalone_and_default_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    fs::write(&native, include_bytes!("../../../fixtures/assembly/calibration.viamind.json")).unwrap();
    let original = fs::read(&native).unwrap();
    let mut job = viamind_fab::OutputJob::new("CLI companion".into(), 0.);
    job.version = 2;
    job.include_calibrated_placement = Some(true);
    let job_path = temp.path().join("job.json");
    viamind_fab::save_output_job(&job, &job_path, None).unwrap();
    let release = temp.path().join("release");
    let run = |path: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_viamind"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .args(["--run-output-job"])
            .arg(&job_path)
            .arg("--open")
            .arg(&native)
            .arg("--output")
            .arg(path)
            .output()
            .unwrap()
    };
    let result = run(&release);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(release.join("release-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], 2);
    assert_eq!(manifest["calibrated_placement"]["component_count"], 2);
    let standalone = temp.path().join("standalone.csv");
    let result = Command::new(env!("CARGO_BIN_EXE_viamind"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .args(["--export-csv", "assembly_placement", "--open"])
        .arg(&native)
        .arg("--output")
        .arg(&standalone)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(fs::read(standalone).unwrap(), fs::read(release.join("assembly/calibrated-placement.csv")).unwrap());
    assert!(!run(&release).status.success());
    assert!(!run(&native).status.success());
    assert_eq!(fs::read(&native).unwrap(), original);
    let mut invalid = job;
    invalid.version = 1;
    fs::write(&job_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let refused = temp.path().join("refused");
    assert!(!run(&refused).status.success());
    assert!(!refused.exists());
}
