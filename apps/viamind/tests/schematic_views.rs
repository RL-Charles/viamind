use std::{ffi::OsStr, fs, process::Command};
const SHEET: &str = "00000000-0000-0000-0000-000000000066";
fn run(args: &[&OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_viamind")).args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").output().unwrap()
}
#[test]
fn selected_sheet_svg_png_work_without_display_and_protect_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("schematic.viamind.json");
    let source = include_bytes!("../../../fixtures/views/schematic.viamind.json");
    fs::write(&native, source).unwrap();
    for (mode, suffix) in [("--export-schematic-svg", "svg"), ("--export-schematic-png", "png")] {
        let out = dir.path().join(format!("sheet.{suffix}"));
        let args = [
            mode.as_ref(),
            "--sheet".as_ref(),
            SHEET.as_ref(),
            "--open".as_ref(),
            native.as_os_str(),
            "--output".as_ref(),
            out.as_os_str(),
            "--view-size".as_ref(),
            "600,400".as_ref(),
            "--view-bounds".as_ref(),
            "0,0,30,20".as_ref(),
        ];
        let result = run(&args);
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let bytes = fs::read(&out).unwrap();
        if suffix == "png" {
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 600);
        } else {
            let svg = String::from_utf8(bytes.clone()).unwrap();
            assert!(!svg.contains("<text"));
            assert!(svg.contains("10kΩ µA é Я"));
        }
        assert!(!run(&args).status.success());
        assert_eq!(fs::read(&out).unwrap(), bytes);
        assert!(
            !run(&[
                mode.as_ref(),
                "--sheet".as_ref(),
                SHEET.as_ref(),
                "--open".as_ref(),
                native.as_os_str(),
                "--output".as_ref(),
                native.as_os_str()
            ])
            .status
            .success()
        );
        assert_eq!(fs::read(&native).unwrap(), source);
        for (flag, value) in [
            ("--sheet", "bad"),
            ("--sheet", "00000000-0000-0000-0000-000000000099"),
            ("--view-size", "4096,4096"),
            ("--view-layers", "top_copper"),
            ("--schematic-layers", "bad"),
        ] {
            let invalid = dir.path().join(format!("invalid-{suffix}"));
            let base: [&OsStr; 5] = [mode.as_ref(), "--open".as_ref(), native.as_os_str(), "--output".as_ref(), invalid.as_os_str()];
            let mut args = base.to_vec();
            if flag != "--sheet" {
                args.extend([OsStr::new("--sheet"), OsStr::new(SHEET)]);
            }
            args.extend([OsStr::new(flag), OsStr::new(value)]);
            assert!(!run(&args).status.success());
            assert!(!invalid.exists());
        }
    }
    assert!(!run(&["--sheet".as_ref(), SHEET.as_ref()]).status.success());
    assert!(!run(&["--schematic-layers".as_ref(), "wires".as_ref()]).status.success());
}
