use serde_json::{Value, json};
use viamind_doc::{
    file,
    schematic::{Coordinate, PinType},
    symbol_library::{self, SymbolTemplate, TemplatePin},
};
use viamind_geom::Nm;

fn template() -> SymbolTemplate {
    SymbolTemplate {
        name: "Original one-pin sensor".into(),
        value: "1 kΩ".into(),
        footprint: Some("Original-SMD".into()),
        width: Nm(3_000_001),
        height: Nm(5_000_003),
        pins: vec![TemplatePin { number: "1".into(), name: "OUT".into(), position: Coordinate { x: Nm(3), y: Nm(5) }, electrical: PinType::Output }],
    }
}

#[test]
fn detached_roundtrip_preserves_definitions_and_excludes_every_instance_field() {
    let project = file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
    let mut symbol =
        project.schematic.sheets.values().find_map(|s| s.content.as_ref()).unwrap().symbols.values().find(|s| s.designator == "U1").unwrap().clone();
    let original = symbol_library::SymbolTemplate::from_symbol("Original regulator", &symbol).unwrap();
    let bytes = symbol_library::to_vec(&original).unwrap();
    let data: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(data["format"], "viamind-symbol");
    assert_eq!(data["version"], 1);
    assert_eq!(data["symbol"]["pins"][0]["electrical"], "power_input");
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains(&symbol.id.to_string()));
    for pin in &symbol.pins {
        assert!(!text.contains(&pin.id.to_string()));
    }
    for key in ["id", "designator", "net", "rotation_quarters", "mirrored", "schematic_symbol"] {
        assert!(!data["symbol"].as_object().unwrap().contains_key(key));
    }
    symbol.designator = "U999".into();
    symbol.position = Coordinate { x: Nm(50_000_000), y: Nm(20_000_000) };
    symbol.rotation_quarters = 3;
    symbol.mirrored = true;
    assert_eq!(symbol_library::to_vec(&SymbolTemplate::from_symbol("Original regulator", &symbol).unwrap()).unwrap(), bytes);
    assert_eq!(symbol_library::from_slice(&bytes).unwrap(), original);
    assert_eq!(bytes.last(), Some(&b'\n'));
}

#[test]
fn strict_envelopes_and_definition_limits_reject_design_fields_and_malformed_templates() {
    let base: Value = serde_json::from_slice(&symbol_library::to_vec(&template()).unwrap()).unwrap();
    for (pointer, field) in [("", "id"), ("/symbol", "designator"), ("/symbol/pins/0", "id"), ("/symbol/pins/0/position", "net")] {
        let mut data = base.clone();
        data.pointer_mut(pointer).unwrap().as_object_mut().unwrap().insert(field.into(), json!("forbidden"));
        assert!(symbol_library::from_slice(&serde_json::to_vec(&data).unwrap()).is_err());
    }
    for (pointer, value) in [
        ("/format", json!("other")),
        ("/version", json!(2)),
        ("/symbol/width", json!(0)),
        ("/symbol/pins/0/position/x", json!(10_000_000_001i64)),
        ("/symbol/pins/0/number", json!(" 1")),
        ("/symbol/pins/0/name", json!("bad\nname")),
    ] {
        let mut data = base.clone();
        *data.pointer_mut(pointer).unwrap() = value;
        assert!(symbol_library::from_slice(&serde_json::to_vec(&data).unwrap()).is_err());
    }
    let mut duplicate = template();
    duplicate.pins.push(duplicate.pins[0].clone());
    assert!(duplicate.validate().is_err());
    let mut count = template();
    count.pins = vec![count.pins[0].clone(); 1025];
    assert!(count.validate().unwrap_err().contains("1024"));
    assert!(symbol_library::from_slice(&vec![b' '; symbol_library::MAX_BYTES + 1]).unwrap_err().contains("1 MiB"));
}

#[test]
fn utf8_field_and_aggregate_budgets_are_checked_before_source_copy() {
    let mut t = template();
    t.value = "😀".repeat(1024);
    assert!(t.validate().is_ok());
    t.value.push('x');
    assert!(t.validate().unwrap_err().contains("4096"));
    t = template();
    t.pins = (0..1024)
        .map(|n| TemplatePin { number: n.to_string(), name: "😀".repeat(128), position: Coordinate::default(), electrical: PinType::Passive })
        .collect();
    assert!(t.validate().unwrap_err().contains("256 KiB"));
    let mut source = template().instantiate("S1", Coordinate::default(), 0, false).unwrap();
    source.pins[0].name = "x".repeat(4097);
    assert!(SymbolTemplate::from_symbol("source", &source).unwrap_err().contains("4096"));
}

#[test]
fn exact_mirrored_quarter_turn_and_odd_body_bounds_have_fresh_identities() {
    let t = template();
    let pos = Coordinate { x: Nm(100), y: Nm(200) };
    let a = t.instantiate("S1", pos, 1, true).unwrap();
    let b = t.instantiate("S2", pos, 1, true).unwrap();
    assert_eq!(a.pin_position(a.pins[0].position).unwrap(), Coordinate { x: Nm(105), y: Nm(203) });
    assert_ne!(a.id, b.id);
    assert_ne!(a.pins[0].id, b.pins[0].id);
    assert_ne!(a.id, a.pins[0].id);
    assert!(t.instantiate("S3", pos, 4, false).is_err());
    let mut body = t;
    body.pins.clear();
    body.width = Nm(1);
    body.height = Nm(1);
    assert!(body.instantiate("S3", Coordinate { x: Nm(10_000_000_000), y: Nm(0) }, 0, false).is_err());
    body.width = Nm(2);
    assert!(body.instantiate("S3", Coordinate { x: Nm(9_999_999_999), y: Nm(0) }, 0, false).is_ok());
}

#[test]
fn bounded_regular_file_loading_and_atomic_new_publication_preserve_existing_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sensor.symbol.json");
    let t = template();
    assert_eq!(symbol_library::save_new(&t, &path, None).unwrap(), path);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(symbol_library::load(&path).unwrap(), t);
    assert!(symbol_library::save_new(&t, &path, None).is_err());
    assert!(symbol_library::save_new(&t, &path, Some(&path)).unwrap_err().contains("active native"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(symbol_library::load(directory.path()).unwrap_err().contains("regular file"));
    let huge = directory.path().join("huge.json");
    std::fs::write(&huge, vec![b' '; symbol_library::MAX_BYTES + 1]).unwrap();
    assert!(symbol_library::load(&huge).unwrap_err().contains("1 MiB"));
    let mut bad = t;
    bad.width = Nm(0);
    assert!(symbol_library::save_new(&bad, &directory.path().join("bad.json"), None).is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2, "no staged or partial outputs remain");
}
