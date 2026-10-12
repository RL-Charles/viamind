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
fn bus_glyph_refusal_is_layer_specific() {
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/buses.viamind.json")).unwrap();
    let sheet = *p.schematic.sheets.keys().next().unwrap();
    p.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap().buses.values_mut().next().unwrap().expression = "漢[0..1]".into();
    let snapshot = schematic_view_snapshot(&p, sheet).unwrap();
    assert!(schematic_svg(&snapshot, &SchematicViewOptions::default()).unwrap_err().to_string().contains("U+6F22"));
    assert!(schematic_svg(&snapshot, &SchematicViewOptions { layers: vec![SchematicLayer::Wires], ..Default::default() }).is_ok());
}
#[test]
fn diagonal_graphic_entry_and_escaped_expression_have_independent_bounds() {
    use viamind_doc::{
        SchematicBusEntry,
        schematic::{Coordinate, LabelScope, SchematicLabel, SchematicWire},
    };
    use viamind_geom::Nm;
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/buses.viamind.json")).unwrap();
    let sheet = *p.schematic.sheets.keys().next().unwrap();
    let c = p.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
    let bus = *c.buses.keys().next().unwrap();
    c.buses.get_mut(&bus).unwrap().expression = "<&>[1..0]".into();
    let entry = ObjectId::new();
    let wire = ObjectId::new();
    let label = ObjectId::new();
    let point = |x, y| Coordinate { x: Nm(x), y: Nm(y) };
    c.wires.insert(wire, SchematicWire { id: wire, start: point(-2_000_000, 2_000_000), end: point(-2_000_000, 8_000_000) });
    c.labels.insert(label, SchematicLabel { id: label, position: point(-2_000_000, 2_000_000), name: "<&>0".into(), scope: LabelScope::Local });
    c.bus_entries.insert(
        entry,
        SchematicBusEntry {
            id: entry,
            bus_id: bus,
            member: "<&>0".into(),
            wire_id: wire,
            label_id: label,
            bus_point: point(0, 0),
            wire_point: point(-2_000_000, 2_000_000),
        },
    );
    let snapshot = schematic_view_snapshot(&p, sheet).unwrap();
    let r = schematic_svg(&snapshot, &SchematicViewOptions { layers: vec![SchematicLayer::Buses], padding_nm: 0, ..Default::default() }).unwrap();
    assert_eq!(r.metadata.bus_entry_count, 1);
    assert!(r.svg.contains(&format!("bus-entry-{entry}")));
    assert!(r.svg.contains("&lt;&amp;&gt;[1..0]"));
    assert!(!r.svg.contains("<&>"));
    assert!(!r.svg.contains(&format!("wire-{wire}")));
    assert!(r.metadata.region_nm.min_x <= -2_125_000);
    assert!(r.metadata.region_nm.max_y >= 2_125_000);
    assert!(r.metadata.region_nm.min_y <= -2_000_000);
}
