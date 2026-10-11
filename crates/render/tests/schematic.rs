use viamind_doc::{ObjectId, Project};
use viamind_render::{SchematicLayer, SchematicViewOptions, ViewBounds, schematic_png, schematic_svg, schematic_view_snapshot};
fn fixture() -> (Project, ObjectId) {
    let p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    let id = *p.schematic.sheets.keys().next().unwrap();
    (p, id)
}
#[test]
fn outlined_text_geometry_is_deterministic_and_snapshot_excludes_unrelated_data() {
    let (mut p, id) = fixture();
    let snapshot = schematic_view_snapshot(&p, id).unwrap();
    let options = SchematicViewOptions::default();
    let svg = schematic_svg(&snapshot, &options).unwrap();
    let png = schematic_png(&snapshot, &options).unwrap();
    assert!(svg.svg.contains("<path"));
    assert!(!svg.svg.contains("<text"));
    assert!(!svg.svg.contains("font-family"));
    assert!(svg.svg.contains("U&lt;&amp;&gt;1"));
    assert!(svg.svg.contains("10kΩ µA é Я"));
    assert!(svg.svg.contains("BITSTREAM VERA LICENSE"));
    assert_eq!(svg.metadata.pin_count, 2);
    assert_eq!(svg.metadata.wire_count, 2);
    assert_eq!(svg.metadata.junction_count, 1);
    assert_eq!(svg.svg, schematic_svg(&snapshot, &options).unwrap().svg);
    assert_eq!(png.png, schematic_png(&snapshot, &options).unwrap().png);
    // An invalid giant unrelated board and malformed other sheet are irrelevant to this sheet.
    let mut other = viamind_doc::Sheet::new("X".repeat(9 * 1024 * 1024));
    other.id = p.id;
    p.schematic.sheets.insert(ObjectId::new(), other);
    let mut board = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap().board;
    board.parts.values_mut().next().unwrap().value = "Y".repeat(9 * 1024 * 1024);
    p.board = board;
    let isolated = schematic_view_snapshot(&p, id).unwrap();
    assert_eq!(svg.svg, schematic_svg(&isolated, &options).unwrap().svg);
    assert_eq!(png.png, schematic_png(&isolated, &options).unwrap().png);
    assert!(format!("{isolated:?}").len() < 10000);
}
#[test]
fn filters_regions_legacy_and_invalid_selected_inputs_are_explicit() {
    let (mut p, id) = fixture();
    let snapshot = schematic_view_snapshot(&p, id).unwrap();
    let options = SchematicViewOptions {
        layers: vec![SchematicLayer::Wires, SchematicLayer::Wires],
        region_nm: Some(ViewBounds { min_x: 0, min_y: 0, max_x: 30_000_000, max_y: 20_000_000 }),
        width_px: 600,
        height_px: 400,
        ..Default::default()
    };
    let svg = schematic_svg(&snapshot, &options).unwrap();
    assert_eq!(svg.metadata.layers, vec![SchematicLayer::Wires]);
    assert!(!svg.svg.contains("pin-stem-"));
    assert!(svg.svg.contains("wire-"));
    let missing = ObjectId::new();
    assert!(schematic_view_snapshot(&p, missing).is_err());
    p.schematic.sheets.get_mut(&id).unwrap().content.as_mut().unwrap().symbols.values_mut().next().unwrap().value = "漢".into();
    let snapshot = schematic_view_snapshot(&p, id).unwrap();
    assert!(schematic_svg(&snapshot, &SchematicViewOptions::default()).unwrap_err().to_string().contains("U+6F22"));
    // Unsupported glyphs in omitted categories do not affect an explicit wires-only view.
    assert!(schematic_svg(&snapshot, &options).is_ok());
    p.schematic.sheets.get_mut(&id).unwrap().content = None;
    let empty = schematic_svg(&schematic_view_snapshot(&p, id).unwrap(), &SchematicViewOptions::default()).unwrap();
    assert!(empty.metadata.legacy_empty_sheet);
    assert!(!empty.svg.contains("wire-"));
    p.schematic.sheets.get_mut(&id).unwrap().id = ObjectId::new();
    assert!(schematic_view_snapshot(&p, id).is_err());
}
#[test]
fn all_budgets_fail_before_clone_or_publication() {
    let (mut p, id) = fixture();
    p.name = "x".repeat(4097);
    assert!(schematic_view_snapshot(&p, id).is_err());
    p.name = "small".into();
    let c = p.schematic.sheets.get_mut(&id).unwrap().content.as_mut().unwrap();
    let template = c.labels.values().next().unwrap().clone();
    for _ in 0..1000 {
        let mut l = template.clone();
        l.id = ObjectId::new();
        l.name = "A".repeat(128);
        c.labels.insert(l.id, l);
    }
    let before = p.clone();
    assert!(schematic_view_snapshot(&p, id).unwrap_err().to_string().contains("glyph"));
    assert_eq!(p, before);
    for options in [
        SchematicViewOptions { width_px: 4096, height_px: 4096, ..Default::default() },
        SchematicViewOptions { layers: vec![], ..Default::default() },
        SchematicViewOptions { region_nm: Some(ViewBounds { min_x: 1, min_y: 0, max_x: 0, max_y: 1 }), ..Default::default() },
    ] {
        assert!(options.validate().is_err());
    }
}
#[test]
fn distant_crop_is_pixel_identical_and_extreme_zoom_fails() {
    let (mut p, id) = fixture();
    let region = ViewBounds { min_x: 0, min_y: 0, max_x: 30_000_000, max_y: 20_000_000 };
    let options = SchematicViewOptions { region_nm: Some(region), width_px: 600, height_px: 400, ..Default::default() };
    let near = schematic_png(&schematic_view_snapshot(&p, id).unwrap(), &options).unwrap().png;
    let c = p.schematic.sheets.get_mut(&id).unwrap().content.as_mut().unwrap();
    let shift = 9_000_000_000;
    for s in c.symbols.values_mut() {
        s.position.x.0 += shift;
        s.position.y.0 += shift;
    }
    for w in c.wires.values_mut() {
        w.start.x.0 += shift;
        w.start.y.0 += shift;
        w.end.x.0 += shift;
        w.end.y.0 += shift;
    }
    for j in c.junctions.values_mut() {
        j.position.x.0 += shift;
        j.position.y.0 += shift;
    }
    for l in c.labels.values_mut() {
        l.position.x.0 += shift;
        l.position.y.0 += shift;
    }
    let options = SchematicViewOptions {
        region_nm: Some(ViewBounds { min_x: shift, min_y: shift, max_x: shift + 30_000_000, max_y: shift + 20_000_000 }),
        ..options
    };
    assert_eq!(near, schematic_png(&schematic_view_snapshot(&p, id).unwrap(), &options).unwrap().png);
    let tiny = SchematicViewOptions {
        region_nm: Some(ViewBounds { min_x: shift + 16_000_000, min_y: shift + 7_000_000, max_x: shift + 16_000_001, max_y: shift + 7_000_001 }),
        ..options
    };
    assert!(schematic_svg(&schematic_view_snapshot(&p, id).unwrap(), &tiny).unwrap_err().to_string().contains("precision"));
}
