//! Independent authored DXF fixtures, unit conversion and strict failure paths.
use viamind_doc::dxf::{DxfUnits, MAX_DXF_BYTES, import_outline_dxf};
use viamind_geom::Nm;
const MM: &str = include_str!("fixtures/outline-mm.dxf");
fn parse(text: &str) -> Result<viamind_doc::BoardOutline, viamind_doc::dxf::DxfError> {
    import_outline_dxf(text, DxfUnits::Header, None)
}
#[test]
fn import_classifies_polylines_and_assembles_unordered_lines() {
    let outline = parse(MM).unwrap();
    assert_eq!(outline.outer.points.len(), 4);
    assert_eq!(outline.cutouts.len(), 1);
    assert_eq!(outline.outer.points[2].x, Nm::from_mm(40.0));
    assert_eq!(outline.outer.points[2].y, Nm::from_mm(-30.0));
    let lines = parse(include_str!("fixtures/outline-lines.dxf")).unwrap();
    let polyline = parse(include_str!("fixtures/outline-polyline.dxf")).unwrap();
    assert_eq!(lines.outer.signed_area_x2().abs(), outline.outer.signed_area_x2().abs());
    assert_eq!(polyline.outer, outline.outer);
    assert_eq!(parse(&MM.replace('\n', "\r\n")).unwrap(), outline);
}
#[test]
fn units_require_header_or_an_explicit_override_and_round_to_nm() {
    let inch = parse(include_str!("fixtures/outline-inch.dxf")).unwrap();
    assert_eq!(inch.outer.points[2].x, Nm(50_800_000));
    let unitless = MM.replace("$INSUNITS\n70\n4", "$INSUNITS\n70\n0");
    assert!(parse(&unitless).unwrap_err().to_string().contains("choose explicit"));
    assert_eq!(import_outline_dxf(&unitless, DxfUnits::Mm, None).unwrap(), parse(MM).unwrap());
    let small = MM.replace("10\n40\n", "10\n40.0000006\n");
    assert_eq!(parse(&small).unwrap().outer.points[1].x, Nm(40_000_001));
    for (unit, scale) in [(5, 10), (6, 1000), (9, 0)] {
        let text = MM.replace("$INSUNITS\n70\n4", &format!("$INSUNITS\n70\n{unit}"));
        if scale == 1000 {
            assert!(parse(&text).is_err()); // oversized drawing after conversion
        } else {
            assert!(parse(&text).is_ok());
        }
    }
}
#[test]
fn malformed_curved_3d_open_and_ambiguous_drawings_fail_with_context() {
    let lines = include_str!("fixtures/outline-lines.dxf");
    let failures = [
        MM.trim_end_matches("0\nEOF\n").to_owned(),
        MM.replace("90\n4", "90\n5"),
        MM.replace("70\n1", "70\n0"),
        MM.replace("70\n1", "70\n9"),
        MM.replace("LWPOLYLINE", "ARC"),
        MM.replace("70\n1", "70\n1\n42\n0.5"),
        MM.replace("70\n1", "70\n1\n38\n1"),
        MM.replace("70\n1", "70\n1\n230\n-1"),
        MM.replace("70\n1", "70\n1\n40\n2"),
        MM.replace("70\n1", "70\n1\n67\n1"),
        MM.replace("10\n40", "10\nNaN"),
        MM.replace("10\n40", "10\n1e100"),
        MM.replace("20\n30", "20\n30\n20\n30"),
        lines.replacen("11\n40", "11\n39", 1),
        lines.replace("11\n40", "11\n40\n11\n40"),
        include_str!("fixtures/outline-polyline.dxf").replace("0\nSEQEND\n", ""),
        include_str!("fixtures/outline-polyline.dxf").replace("0\nVERTEX", "0\nVERTEX\n70\n32"),
        "AutoCAD Binary DXF\0".into(),
        "0\nSECTION\n2".into(),
        format!("{MM}extra\n0\n"),
    ];
    for text in failures {
        let error = parse(&text).expect_err(&text);
        assert!(error.to_string().starts_with("DXF outline:"));
    }
    assert!(parse(&" ".repeat(MAX_DXF_BYTES + 1)).is_err());
}
#[test]
fn layer_filter_and_board_topology_never_silently_drop_selected_geometry() {
    let mixed = MM.replace("0\nENDSEC\n0\nEOF", "0\nTEXT\n8\nNotes\n1\nhello\n0\nENDSEC\n0\nEOF");
    assert!(parse(&mixed).unwrap_err().to_string().contains("unsupported entity TEXT"));
    assert_eq!(import_outline_dxf(&mixed, DxfUnits::Header, Some("Edge")).unwrap(), parse(MM).unwrap());
    assert!(import_outline_dxf(MM, DxfUnits::Header, Some("missing")).is_err());
    assert!(import_outline_dxf(MM, DxfUnits::Header, Some("")).is_err());
    // Disjoint and touching rings are rejected by the common substrate validation.
    assert!(parse(&MM.replace("10\n10", "10\n50").replace("10\n20", "10\n60")).is_err());
    assert!(parse(&MM.replace("10\n10", "10\n0")).is_err());
    assert!(parse(&MM.replace("10\n40\n20\n0", "10\n0\n20\n30")).is_err());
}

#[test]
fn asymmetric_coordinates_preserve_dxf_visual_orientation_and_parent_layer_selection() {
    let asymmetric = MM.replace("10\n40\n20\n30", "10\n35\n20\n27");
    let outline = parse(&asymmetric).unwrap();
    assert_eq!(outline.outer.points[2], viamind_geom::Point::new(Nm(35_000_000), Nm(-27_000_000)));
    assert!(outline.outer.signed_area_x2() < 0); // original source winds CCW in +Y-up WCS
    let old = include_str!("fixtures/outline-polyline.dxf");
    let children = old.replace("0\nVERTEX\n", "0\nVERTEX\n8\nOther\n");
    assert_eq!(import_outline_dxf(&children, DxfUnits::Header, Some("Edge")).unwrap(), parse(old).unwrap());
    assert!(parse(&old.replace("66\n1", "66\n1\n10\n2")).unwrap_err().to_string().contains("dummy"));
}
