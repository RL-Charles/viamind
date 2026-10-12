use viamind_doc::ObjectId;
use viamind_render::{SchematicLayer, SchematicViewOptions, schematic_png, schematic_svg, schematic_view_snapshot};
#[test]
fn explicit_bus_layer_declares_graphic_counts_and_contains_expression_ink_in_fit() {
    let p = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/buses.viamind.json")).unwrap();
    let sheet: ObjectId = "00000000-0000-0000-0000-000000001271".parse().unwrap();
    let snapshot = schematic_view_snapshot(&p, sheet).unwrap();
    let options = SchematicViewOptions { layers: vec![SchematicLayer::Buses], padding_nm: 0, ..Default::default() };
    let rendered = schematic_svg(&snapshot, &options).unwrap();
    assert_eq!(rendered.metadata.layers, [SchematicLayer::Buses]);
    assert_eq!((rendered.metadata.bus_count, rendered.metadata.bus_entry_count), (1, 0));
    assert!(rendered.svg.contains("bus-expression-00000000-0000-0000-0000-000000001272"));
    assert!(rendered.svg.contains("DATA[1..0]"));
    assert!(!rendered.svg.contains("wire-00000000-0000-0000-0000-000000001273"));
    assert!(!rendered.svg.contains("<text"));
    assert!(rendered.metadata.region_nm.min_y <= -2_000_000);
    assert!(rendered.metadata.region_nm.max_x >= 20_200_000);
    assert_eq!(rendered.svg, schematic_svg(&snapshot, &options).unwrap().svg);
    let png = schematic_png(&snapshot, &options).unwrap();
    assert_eq!(png.png, schematic_png(&snapshot, &options).unwrap().png);
    let wires = schematic_svg(&snapshot, &SchematicViewOptions { layers: vec![SchematicLayer::Wires], ..Default::default() }).unwrap();
    assert!(!wires.svg.contains("bus-expression-"));
    assert!(wires.svg.contains("wire-00000000-0000-0000-0000-000000001273"));
}
#[test]
fn bus_glyph_refusal_is_layer_specific_and_native_entry_geometry_is_exact() {
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/buses.viamind.json")).unwrap();
    let sheet = *p.schematic.sheets.keys().next().unwrap();
    p.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap().buses.values_mut().next().unwrap().expression = "漢[0..1]".into();
    let snapshot = schematic_view_snapshot(&p, sheet).unwrap();
    assert!(schematic_svg(&snapshot, &SchematicViewOptions::default()).unwrap_err().to_string().contains("U+6F22"));
    assert!(schematic_svg(&snapshot, &SchematicViewOptions { layers: vec![SchematicLayer::Wires], ..Default::default() }).is_ok());
}
