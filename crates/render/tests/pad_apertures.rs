use viamind_doc::{ApertureRect, PadApertures, Side, file};
use viamind_geom::{Nm, Point};
use viamind_render::{SvgOptions, ViewLayer, board_svg};
#[test]
fn selected_mask_paste_preserve_odd_edges_roles_omissions_and_default_bytes() {
    let mut p = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let original = board_svg(&p, &SvgOptions::default()).unwrap();
    assert_eq!(original.svg, include_str!("../../../fixtures/fabrication/legacy-aperture-baseline/board.svg"));
    let id = *p.board.parts.keys().next().unwrap();
    let part = p.board.parts.get_mut(&id).unwrap();
    let pad_id = part.pads.as_ref().unwrap().0[0].id;
    part.pads.as_mut().unwrap().0[0].apertures = Some(PadApertures { mask: Some(ApertureRect { width: Nm(3), height: Nm(5) }), paste: None });
    assert_eq!(board_svg(&p, &SvgOptions::default()).unwrap().svg, original.svg);
    assert!(serde_json::to_value(&original.metadata).unwrap().get("pad_apertures").is_none());
    p.board.parts.retain(|key, _| *key == id);
    p.board.vias.clear();
    p.board.tracks.clear();
    let part = p.board.parts.get_mut(&id).unwrap();
    part.position = Point::new(Nm(10_000_000), Nm(20_000_000));
    part.side = Side::Bottom;
    part.rotation_mdeg = 90000;
    part.pads.as_mut().unwrap().0.truncate(1);
    part.pads.as_mut().unwrap().0[0].position = Point::new(Nm(3), Nm(5));
    let options = SvgOptions { layers: vec![ViewLayer::BottomMask, ViewLayer::BottomPaste], padding_nm: 0, ..Default::default() };
    let view = board_svg(&p, &options).unwrap();
    assert!(view.svg.contains(&format!("pad-{pad_id}-mask")));
    assert!(!view.svg.contains(&format!("pad-{pad_id}-paste")));
    assert!(view.svg.contains("translate(10.0000050 20.0000030) rotate(-90.000)"));
    assert!(view.svg.contains("x=\"-0.0000015\" y=\"-0.0000025\" width=\"0.0000030\" height=\"0.0000050\""));
    let rows = &view.metadata.pad_apertures.unwrap().pads;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pad_id, pad_id);
    assert_eq!(rows[0].mask, "explicit_rectangle");
    assert_eq!(rows[0].paste, "intentional_omit");
    assert_eq!((view.metadata.region_nm.min_x, view.metadata.region_nm.max_x), (10_000_002, 10_000_008));
}
