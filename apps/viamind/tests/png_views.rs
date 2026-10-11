use std::{ffi::OsStr, fs, process::Command};
fn run(args: &[&OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_viamind")).args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").output().unwrap()
}
#[test]
fn cli_png_renders_without_display_and_preserves_collisions_native_and_invalid_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("board.viamind.json");
    let out = dir.path().join("view.png");
    let bytes = include_bytes!("../../../fixtures/views/asymmetric.viamind.json");
    fs::write(&native, bytes).unwrap();
    let args = [
        "--export-png".as_ref(),
        "--open".as_ref(),
        native.as_os_str(),
        "--output".as_ref(),
        out.as_os_str(),
        "--view-layers".as_ref(),
        "top_copper".as_ref(),
        "--view-bounds".as_ref(),
        "10,5,20,15".as_ref(),
        "--view-size".as_ref(),
        "800,600".as_ref(),
    ];
    let result = run(&args);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let png = fs::read(&out).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 800);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 600);
    assert!(!run(&args).status.success());
    assert_eq!(fs::read(&out).unwrap(), png);
    assert!(!run(&["--export-png".as_ref(), "--open".as_ref(), native.as_os_str(), "--output".as_ref(), native.as_os_str()]).status.success());
    assert_eq!(fs::read(&native).unwrap(), bytes);
    let invalid = dir.path().join("invalid.png");
    for (flag, value) in [
        ("--view-bounds", "NaN,0,1,1"),
        ("--view-bounds", "0,0,0,1"),
        ("--view-size", "5000,1"),
        ("--view-size", "4096,4096"),
        ("--view-layers", "inner_copper"),
        ("--view-padding-mm", "-1"),
    ] {
        assert!(
            !run(&[
                "--export-png".as_ref(),
                "--open".as_ref(),
                native.as_os_str(),
                "--output".as_ref(),
                invalid.as_os_str(),
                flag.as_ref(),
                value.as_ref()
            ])
            .status
            .success()
        );
        assert!(!invalid.exists());
    }
    assert!(!run(&["--view-size".as_ref(), "800,600".as_ref()]).status.success());
    for mode in ["--export-gerber", "--export-csv", "--export-svg"] {
        let mut mixed = args.to_vec();
        mixed.push(mode.as_ref());
        if mode == "--export-csv" {
            mixed.push("bom".as_ref());
        }
        assert!(!run(&mixed).status.success());
        assert_eq!(fs::read(&out).unwrap(), png);
    }
    let mut overwrite = args.to_vec();
    overwrite.push("--overwrite".as_ref());
    assert!(!run(&overwrite).status.success());
}
