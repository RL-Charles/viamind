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
