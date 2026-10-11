use std::{fs, process::Command};
#[test]
fn detached_inspector_is_display_free_json_and_rejects_other_modes_without_writes() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/output-job-population.viamind.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut job = viamind_fab::OutputJob::new("CLI historical release".into(), 0.);
    job.version = 3;
    job.include_calibrated_placement = Some(true);
    job.include_populated_bom = Some(true);
    let path = temp.path().join("release");
    let cursor = viamind_fab::ReleaseCursor { stream_id: "00000000-0000-0000-0000-000000000042".parse().unwrap(), sequence: 9 };
    viamind_fab::OutputJobSnapshot::capture(&project, job).unwrap().write(cursor, &path, None).unwrap();
    let before = fs::read(path.join("release-manifest.json")).unwrap();
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_viamind"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .arg("--inspect-output-release")
            .arg(&path)
            .args(extra)
            .output()
            .unwrap()
    };
    let result = run(&[]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        report,
        serde_json::to_value(viamind_fab::inspect_output_release(&viamind_fab::InspectOutputRelease { path: path.clone() }).unwrap()).unwrap()
    );
    assert_eq!(report["exported_cursor"]["sequence"], 9);
    for extra in [&["--headless"][..], &["--open", "missing"][..], &["--output", "forbidden"][..], &["--agent-read-only"][..]] {
        let result = run(extra);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
    assert_eq!(fs::read(path.join("release-manifest.json")).unwrap(), before);
}
