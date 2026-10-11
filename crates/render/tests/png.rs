use std::{fs, io::Cursor};
use viamind_doc::{Project, file};
use viamind_geom::{Nm, Point};
use viamind_render::{SvgOptions, ViewBounds, ViewLayer, board_png, write_png};
fn sample() -> Project {
    file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap()
}
fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    pixels.truncate(info.buffer_size());
    (info.width, info.height, pixels)
}
fn pixel(pixels: &[u8], w: u32, x: u32, y: u32) -> &[u8] {
    let i = ((y * w + x) * 4) as usize;
    &pixels[i..i + 4]
}
#[test]
fn png_decodes_deterministically_with_shared_layers_physical_holes_cutouts_and_margins() {
    let p = sample();
    let options = SvgOptions {
        region_nm: Some(ViewBounds { min_x: 0, min_y: 0, max_x: 60_000_000, max_y: 40_000_000 }),
        width_px: 1200,
        height_px: 800,
        ..Default::default()
    };
    let output = board_png(&p, &options).unwrap();
    assert_eq!(output.png, board_png(&p, &options).unwrap().png);
    let (w, h, rgba) = decode(&output.png);
    assert_eq!((w, h), (1200, 800));
    assert_eq!(pixel(&rgba, w, 200, 200), [37, 48, 43, 255]);
    // Each via's physical hole punches substrate and overlapping copper.
    for via in p.board.vias.values() {
        let x = (via.position.x.to_mm() * 20.0) as u32;
        let y = (via.position.y.to_mm() * 20.0) as u32;
        assert_eq!(pixel(&rgba, w, x, y)[3], 0);
    }
    let top = board_png(&p, &SvgOptions { layers: vec![ViewLayer::TopCopper], ..options.clone() }).unwrap();
    let (_, _, top) = decode(&top.png);
    assert_eq!(pixel(&top, w, 200, 200)[3], 0);
    let empty = board_png(&Project::new("Empty"), &SvgOptions { width_px: 32, height_px: 16, ..Default::default() }).unwrap();
    assert!(decode(&empty.png).2.as_chunks::<4>().0.iter().all(|p| p[3] == 0));
    assert_eq!(output.metadata.region_nm, options.region_nm.unwrap());
}
#[test]
fn tiny_far_origin_crop_matches_near_origin_and_extreme_large_geometry_rejects() {
    let mut p = sample();
    p.board.tracks.clear();
    p.board.vias.clear();
    p.board.outline = None;
    let mut part = p.board.parts.values().next().unwrap().clone();
    p.board.parts.clear();
    part.rotation_mdeg = 0;
    let pads = part.pads.as_mut().unwrap();
    pads.0.truncate(1);
    pads.0[0].position = Point::ORIGIN;
    pads.0[0].width = Nm(10_000);
    pads.0[0].height = Nm(10_000);
    part.position = Point { x: Nm(10_000), y: Nm(10_000) };
    p.board.parts.insert(part.id, part.clone());
    let near = SvgOptions {
        region_nm: Some(ViewBounds { min_x: 0, min_y: 0, max_x: 20_000, max_y: 20_000 }),
        width_px: 1000,
        height_px: 1000,
        layers: vec![ViewLayer::BottomCopper],
        ..Default::default()
    };
    let a = board_png(&p, &near).unwrap();
    let offset = 99_999_980_000;
    part.position.x.0 += offset;
    part.position.y.0 += offset;
    p.board.parts.insert(part.id, part);
    let far = SvgOptions { region_nm: Some(ViewBounds { min_x: offset, min_y: offset, max_x: offset + 20_000, max_y: offset + 20_000 }), ..near };
    let b = board_png(&p, &far).unwrap();
    assert_eq!(decode(&a.png).2, decode(&b.png).2);
    assert_eq!(b.metadata.region_nm, far.region_nm.unwrap());
    let (_, _, pixels) = decode(&b.png);
    assert_eq!(pixel(&pixels, 1000, 249, 500)[3], 0);
    assert_eq!(pixel(&pixels, 1000, 250, 500), [71, 115, 220, 255]);
    assert_eq!(pixel(&pixels, 1000, 749, 500)[3], 255);
    assert_eq!(pixel(&pixels, 1000, 750, 500)[3], 0);
    let part = p.board.parts.values_mut().next().unwrap();
    part.position = Point { x: Nm(10_000), y: Nm(10_000) };
    part.pads.as_mut().unwrap().0[0].width = Nm(100_000_000);
    let zoom = SvgOptions { region_nm: Some(ViewBounds { min_x: 0, min_y: 0, max_x: 20_000, max_y: 20_000 }), ..far };
    assert!(board_png(&p, &zoom).unwrap_err().to_string().contains("precision budget"));
}
#[test]
fn pixel_limits_and_atomic_png_publication_preserve_native_and_collisions() {
    assert!(board_png(&sample(), &SvgOptions { width_px: 4096, height_px: 4096, ..Default::default() }).unwrap_err().to_string().contains("4194304"));
    let output = board_png(&sample(), &SvgOptions { width_px: 100, height_px: 100, ..Default::default() }).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.png");
    fs::write(&path, b"native").unwrap();
    assert!(write_png(&output.png, &path, Some(&path)).unwrap_err().to_string().contains("active native"));
    assert_eq!(fs::read(&path).unwrap(), b"native");
    assert!(write_png(&output.png, &path, None).is_err());
    fs::remove_file(&path).unwrap();
    write_png(&output.png, &path, None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), output.png);
    assert!(write_png(&vec![0; viamind_render::MAX_PNG_BYTES + 1], &dir.path().join("large.png"), None).is_err());
    assert!(!dir.path().join("large.png").exists());
}

#[test]
fn board_only_capture_excludes_unrelated_schematic_and_bounds_text_before_copy() {
    let mut project = sample();
    let mut sheet = viamind_doc::Sheet::new("Unrelated");
    sheet.name = "s".repeat(8 * 1024 * 1024);
    // Deliberately invalid/unrelated sheet identity must not affect board views.
    project.schematic.sheets.insert(project.id, sheet);
    let snapshot = viamind_render::board_view_snapshot(&project).unwrap();
    assert!(snapshot.schematic.sheets.is_empty());
    assert_eq!(snapshot.id, project.id);
    assert_eq!(snapshot.name, project.name);
    assert_eq!(snapshot.board, project.board);
    let options = SvgOptions { width_px: 64, height_px: 64, ..Default::default() };
    assert_eq!(viamind_render::board_svg(&project, &options).unwrap().svg, viamind_render::board_svg(&snapshot, &options).unwrap().svg);
    assert_eq!(board_png(&project, &options).unwrap().png, board_png(&snapshot, &options).unwrap().png);
    assert_eq!(board_png(&snapshot, &SvgOptions { width_px: 64, height_px: 64, ..Default::default() }).unwrap().metadata.pad_count, 3);
    let mut part = project.board.parts.values().next().unwrap().clone();
    part.pads = None;
    part.value = "v".repeat(4096);
    project.board.parts.clear();
    for _ in 0..513 {
        part.id = viamind_doc::ObjectId::new();
        project.board.parts.insert(part.id, part.clone());
    }
    let before = project.board.clone();
    assert!(viamind_render::board_view_snapshot(&project).unwrap_err().to_string().contains("aggregate"));
    assert!(viamind_render::board_svg(&project, &SvgOptions::default()).unwrap_err().to_string().contains("aggregate"));
    assert_eq!(project.board, before);
    project.board.parts.values_mut().next().unwrap().value = "x".repeat(4097);
    assert!(viamind_render::board_view_snapshot(&project).unwrap_err().to_string().contains("4096"));
}
