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

#[test]
fn actual_cli_legend_opt_in_and_explicit_artwork_view_layers() {
    use viamind_doc::{FootprintArtwork, SilkscreenSegment, file};
    use viamind_geom::{Nm, Point};
    let temp = tempfile::tempdir().unwrap();
    let mut p = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let part = p.board.parts.values_mut().next().unwrap();
    part.artwork = Some(Box::new(FootprintArtwork {
        silkscreen: vec![SilkscreenSegment { start: Point::new(Nm(-3), Nm(5)), end: Point::new(Nm(7), Nm(11)), width: Nm(3) }],
        courtyard: None,
    }));
    let native = temp.path().join("art.viamind.json");
    file::save_new(&p, &native).unwrap();
    let out = temp.path().join("Legend");
    let output = run(&[
        "--export-gerber".as_ref(),
        "--include-silkscreen".as_ref(),
        "--open".as_ref(),
        native.as_os_str(),
        "--output".as_ref(),
        out.as_os_str(),
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(fs::read_to_string(out.join("top-silkscreen.gbr")).unwrap().contains("Legend,Top"));
    assert!(fs::read_to_string(out.join("bottom-silkscreen.gbr")).unwrap().contains("Legend,Bot"));
    let svg = temp.path().join("art.svg");
    let output = run(&[
        "--export-svg".as_ref(),
        "--view-layers".as_ref(),
        "top_silkscreen,top_courtyard,bottom_silkscreen,bottom_courtyard".as_ref(),
        "--open".as_ref(),
        native.as_os_str(),
        "--output".as_ref(),
        svg.as_os_str(),
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(fs::read_to_string(svg).unwrap().contains("<line "));
    for args in [vec!["--include-silkscreen".as_ref()], vec!["--include-silkscreen".as_ref(), "--export-schematic-svg".as_ref()]] {
        let output = run(&args);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("requires --export-gerber"));
    }
}
