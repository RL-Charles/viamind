use viamind_doc::{FootprintArtwork, RectangularCourtyard, Side, SilkscreenSegment, file};
use viamind_geom::{Nm, Point};
use viamind_render::{SvgOptions, ViewLayer, board_svg};
#[test]
fn explicit_layer_output_exact_quarter_bottom_and_legacy_defaults() {
    let mut p = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let id = *p.board.parts.keys().next().unwrap();
    let baseline = board_svg(&p, &SvgOptions::default()).unwrap();
    let options = SvgOptions { layers: vec![ViewLayer::TopSilkscreen, ViewLayer::TopCourtyard], padding_nm: 0, ..Default::default() };
    let part = p.board.parts.get_mut(&id).unwrap();
    part.position = Point::new(Nm(10_000_000), Nm(20_000_000));
    part.rotation_mdeg = 90_000;
    part.side = Side::Top;
    part.artwork = Some(Box::new(FootprintArtwork {
        silkscreen: vec![SilkscreenSegment { start: Point::new(Nm(-3), Nm(5)), end: Point::new(Nm(7), Nm(11)), width: Nm(3) }],
        courtyard: Some(RectangularCourtyard { min: Point::new(Nm(-13), Nm(-17)), max: Point::new(Nm(19), Nm(23)), width: Nm(5) }),
    }));
    let top = board_svg(&p, &options).unwrap();
    assert!(top.svg.contains("x1=\"10.0000050\" y1=\"20.0000030\" x2=\"10.0000110\" y2=\"19.9999930\""), "{}", top.svg);
    assert!(top.svg.contains("stroke-width=\"0.0000030\""));
    assert_eq!(top.metadata.artwork.as_ref().unwrap().rendered_segments, 5);
    assert_eq!(top.svg.matches("<line ").count(), 5);
    p.board.parts.get_mut(&id).unwrap().side = Side::Bottom;
    let bottom = board_svg(&p, &SvgOptions { layers: vec![ViewLayer::BottomSilkscreen], ..options.clone() }).unwrap();
    assert!(bottom.svg.contains("x1=\"10.0000050\" y1=\"19.9999970\" x2=\"10.0000110\" y2=\"20.0000070\""));
    assert_eq!(bottom.svg.matches("<line ").count(), 1);
    let mut original = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let legacy = board_svg(&original, &SvgOptions::default()).unwrap();
    assert_eq!(legacy.svg, baseline.svg);
    assert!(serde_json::to_value(&legacy.metadata).unwrap().get("artwork").is_none());
    original.board.parts.get_mut(&id).unwrap().artwork = p.board.parts[&id].artwork.clone();
    assert_eq!(board_svg(&original, &SvgOptions::default()).unwrap().svg, baseline.svg, "nonempty artwork must not alter default layer bytes");
    original.board.parts.get_mut(&id).unwrap().artwork = Some(Box::default());
    let explicit = board_svg(&original, &SvgOptions::default()).unwrap();
    assert_eq!(explicit.svg, baseline.svg);
    assert_eq!(explicit.metadata.artwork.unwrap().rendered_segments, 0);
}
