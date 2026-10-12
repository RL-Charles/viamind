use viamind_doc::{Project, Side, file, footprint_holes::PartHole};
use viamind_geom::{Nm, Point};
use viamind_render::{SvgOptions, ViewLayer, board_png, board_svg, board_view_snapshot};
fn sample() -> Project {
    let mut p = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let mut part = p.board.parts.values().next().unwrap().clone();
    p.board.parts.clear();
    part.pads = None;
    part.artwork = None;
    part.position = Point::new(Nm(10_000_000), Nm(20_000_000));
    part.holes =
        Some(vec![PartHole { id: viamind_doc::ObjectId::new(), key: "mount.A-1".into(), center: Point::new(Nm(-3), Nm(5)), diameter: Nm(9) }]);
    p.board.parts.insert(part.id, part);
    p.board.tracks.clear();
    p.board.vias.clear();
    p.board.outline = None;
    p
}
#[test]
fn eight_cardinal_side_poses_resolve_exact_circle_roles_parent_identity_and_odd_bounds() {
    for (side, angle, expected) in [
        (Side::Top, 0, (-3, 5)),
        (Side::Top, 90000, (5, 3)),
        (Side::Top, 180000, (3, -5)),
        (Side::Top, 270000, (-5, -3)),
        (Side::Bottom, 0, (3, 5)),
        (Side::Bottom, 90000, (5, -3)),
        (Side::Bottom, 180000, (-3, -5)),
        (Side::Bottom, 270000, (-5, 3)),
    ] {
        let mut p = sample();
        let part = p.board.parts.values_mut().next().unwrap();
        part.side = side;
        part.rotation_mdeg = angle;
        let parent = part.id;
        let hole = part.holes.as_ref().unwrap()[0].id;
        let options = SvgOptions { layers: vec![ViewLayer::NonplatedHoles], padding_nm: 0, width_px: 32, height_px: 32, ..Default::default() };
        let out = board_svg(&p, &options).unwrap();
        let metadata = out.metadata.nonplated_holes.as_ref().unwrap();
        assert_eq!(metadata.rendered_holes, 1);
        assert_eq!(metadata.holes.len(), 1);
        let row = &metadata.holes[0];
        assert_eq!((row.part_id, row.hole_id, row.key.as_str(), row.role), (parent, hole, "mount.A-1", "nonplated_through_hole"));
        assert_eq!(row.center_nm, Point::new(Nm(10_000_000 + expected.0), Nm(20_000_000 + expected.1)));
        assert_eq!(out.metadata.region_nm.min_x, row.center_nm.x.0 - 5);
        assert_eq!(out.metadata.region_nm.max_x, row.center_nm.x.0 + 5);
        assert_eq!(out.metadata.region_nm.min_y, row.center_nm.y.0 - 5);
        assert_eq!(out.metadata.region_nm.max_y, row.center_nm.y.0 + 5);
        assert!(out.svg.contains("data-role=\"nonplated_through_hole\""));
        assert!(out.svg.contains("r=\"0.0000045\""));
        assert_eq!((out.metadata.pad_count, out.metadata.through_via_count), (0, 0));
        let raster = board_png(&p, &options).unwrap();
        let mut decoder = png::Decoder::new(std::io::Cursor::new(&raster.png)).read_info().unwrap();
        let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
        decoder.next_frame(&mut pixels).unwrap();
        let center = (16 * 32 + 16) * 4;
        assert_eq!(&pixels[center..center + 4], &[22, 26, 30, 255]);
        assert_eq!(board_view_snapshot(&p).unwrap().board.parts[&parent].holes, p.board.parts[&parent].holes);
    }
}
#[test]
fn defaults_show_authored_drills_explicit_hide_preserves_metadata_and_legacy_bytes_stay_exact() {
    let p = sample();
    let visible = board_svg(&p, &Default::default()).unwrap();
    assert_eq!(visible.metadata.nonplated_holes.unwrap().rendered_holes, 1);
    let hidden = board_svg(&p, &SvgOptions { layers: vec![ViewLayer::TopCopper], ..Default::default() }).unwrap();
    assert_eq!(hidden.metadata.nonplated_holes.unwrap().rendered_holes, 0);
    assert!(!hidden.svg.contains("npth-"));
    let mut p = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let old = board_svg(&p, &Default::default()).unwrap();
    assert_eq!(old.svg, include_str!("../../../fixtures/fabrication/legacy-aperture-baseline/board.svg"));
    assert!(serde_json::to_value(&old.metadata).unwrap().get("nonplated_holes").is_none());
    p.board.parts.values_mut().next().unwrap().holes = Some(vec![]);
    let explicit = board_svg(&p, &Default::default()).unwrap();
    assert!(explicit.metadata.nonplated_holes.unwrap().holes.is_empty());
}
