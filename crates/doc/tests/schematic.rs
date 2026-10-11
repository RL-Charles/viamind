use serde_json::{Value, json};
use viamind_doc::{ObjectId, Project, Sheet, file, schematic::*};
use viamind_geom::Nm;
fn id(n: u128) -> ObjectId {
    ObjectId::from_uuid(uuid::Uuid::from_u128(n))
}
fn point(x: i64, y: i64) -> Coordinate {
    Coordinate { x: Nm(x), y: Nm(y) }
}
fn example() -> Project {
    let symbol = SchematicSymbol {
        id: id(3),
        designator: "U1".into(),
        value: "Regulator".into(),
        footprint: Some("SMD_3".into()),
        position: point(12_000_000, 20_000_000),
        rotation_quarters: 0,
        mirrored: false,
        width: Nm(4_000_000),
        height: Nm(6_000_000),
        pins: vec![SymbolPin { id: id(4), number: "1".into(), name: "VIN".into(), position: point(-800001, 123), electrical: PinType::PowerInput }],
    };
    let mut content = SchematicContent::default();
    content.symbols.insert(symbol.id, symbol);
    content.wires.insert(id(5), SchematicWire { id: id(5), start: point(0, 0), end: point(12_000_000, 20_000_000) });
    content.junctions.insert(id(6), SchematicJunction { id: id(6), position: point(0, 0) });
    content.labels.insert(id(7), SchematicLabel { id: id(7), position: point(0, 0), name: "VIN".into(), scope: LabelScope::Power });
    let mut p = Project { id: id(1), ..Project::new("Schematic") };
    p.schematic.sheets.insert(id(2), Sheet { id: id(2), name: "Power".into(), content: Some(content) });
    p
}
#[test]
fn exact_cardinal_and_mirror_pin_transforms_roundtrip_native_v5() {
    let p = example();
    validate_project(&p, None).unwrap();
    let mut symbol = p.schematic.sheets[&id(2)].content.as_ref().unwrap().symbols[&id(3)].clone();
    for (mirrored, offsets) in [
        (false, [(-800001, 123), (123, 800001), (800001, -123), (-123, -800001)]),
        (true, [(800001, 123), (123, -800001), (-800001, -123), (-123, 800001)]),
    ] {
        symbol.mirrored = mirrored;
        for (q, (x, y)) in offsets.into_iter().enumerate() {
            symbol.rotation_quarters = q as u8;
            assert_eq!(symbol.pin_position(symbol.pins[0].position).unwrap(), point(12_000_000 + x, 20_000_000 + y));
        }
    }
    let text = file::to_string(&p);
    assert!(text.contains("\"version\": 5"));
    assert_eq!(file::from_str(&text).unwrap(), p);
    assert_eq!(file::to_string(&file::from_str(&text).unwrap()), text);
    assert!(file::from_str(&text.replace("\"version\": 5", "\"version\": 4")).is_err());
}
#[test]
fn malformed_geometry_keys_global_ids_and_unknown_fields_reject() {
    let value: Value = serde_json::from_str(&file::to_string(&example())).unwrap();
    for alter in [
        |v: &mut Value| {
            v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["symbols"][id(3).to_string()]["rotation_quarters"] = json!(4);
        },
        |v: &mut Value| v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["symbols"][id(3).to_string()]["width"] = json!(0),
        |v: &mut Value| {
            v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["symbols"][id(3).to_string()]["position"]["unexpected"] = json!(1);
        },
        |v: &mut Value| {
            v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["symbols"][id(3).to_string()]["pins"][0]["id"] = json!(id(5));
        },
        |v: &mut Value| v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["symbols"][id(3).to_string()]["id"] = json!(id(1)),
        |v: &mut Value| v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["wires"][id(5).to_string()]["end"] = json!({"x":0,"y":0}),
        |v: &mut Value| v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["labels"][id(7).to_string()]["name"] = json!("\n"),
        |v: &mut Value| v["project"]["schematic"]["sheets"][id(2).to_string()]["content"]["unknown"] = json!([]),
    ] {
        let mut bad = value.clone();
        alter(&mut bad);
        assert!(file::from_str(&bad.to_string()).is_err());
    }
    let mut p = example();
    let c = p.schematic.sheets.get_mut(&id(2)).unwrap().content.as_mut().unwrap();
    c.symbols.get_mut(&id(3)).unwrap().pins[0].id = id(0);
    assert!(validate_project(&p, None).is_err());
    let mut p = example();
    let mut other = p.schematic.sheets[&id(2)].clone();
    other.id = id(8);
    let c = other.content.as_mut().unwrap();
    let mut symbol = c.symbols.remove(&id(3)).unwrap();
    symbol.id = id(9);
    symbol.pins[0].id = id(10);
    c.wires.clear();
    c.labels.clear();
    c.junctions.clear();
    c.symbols.insert(symbol.id, symbol);
    p.schematic.sheets.insert(other.id, other);
    assert!(validate_project(&p, None).unwrap_err().contains("reference"));
}
#[test]
fn bounded_counts_and_transformed_world_range_are_enforced() {
    let mut p = example();
    let c = p.schematic.sheets.get_mut(&id(2)).unwrap().content.as_mut().unwrap();
    let symbol = c.symbols.get_mut(&id(3)).unwrap();
    symbol.pins = vec![symbol.pins[0].clone(); 1025];
    assert!(c.validate().unwrap_err().contains("1024"));
    let mut p = example();
    let c = p.schematic.sheets.get_mut(&id(2)).unwrap().content.as_mut().unwrap();
    c.symbols.get_mut(&id(3)).unwrap().position.x = Nm(10_000_000_000);
    c.symbols.get_mut(&id(3)).unwrap().pins[0].position.x = Nm(1);
    assert!(c.validate().is_err());
    let mut content = SchematicContent::default();
    for n in 1..=10001 {
        content.junctions.insert(id(n), SchematicJunction { id: id(n), position: point(0, 0) });
    }
    assert!(content.validate().unwrap_err().contains("10000"));
}
