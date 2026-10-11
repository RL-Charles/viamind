use std::{fs, process::Command};
fn run(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_viamind")).args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").output().unwrap()
}
#[test]
fn headless_cli_exports_real_package_and_refuses_overwrite_or_invalid_invocation() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("board.viamind.json");
    let out = dir.path().join("fab");
    fs::write(&native, include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let args = [
        "--export-gerber".as_ref(),
        "--open".as_ref(),
        native.as_os_str(),
        "--output".as_ref(),
        out.as_os_str(),
        "--mask-expansion-mm".as_ref(),
        "0.05".as_ref(),
    ];
    let result = run(&args);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["mask_expansion_nm"], 50000);
    assert_eq!(manifest["smd_pad_count"], 3);
    let files = fs::read_to_string(out.join("bottom-copper.gbr")).unwrap();
    assert!(!run(&args).status.success());
    assert_eq!(fs::read_to_string(out.join("bottom-copper.gbr")).unwrap(), files);
    let mut overwrite = args.to_vec();
    overwrite.push("--overwrite".as_ref());
    assert!(!run(&overwrite).status.success());
    assert!(!run(&["--mask-expansion-mm".as_ref(), "1".as_ref()]).status.success());
    assert!(!run(&["--export-gerber".as_ref(), "--open".as_ref(), native.as_os_str(), "--output".as_ref(), native.as_os_str()]).status.success());
    assert!(viamind_doc::file::load(native).is_ok());
}
