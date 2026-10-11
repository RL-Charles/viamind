use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, PinType, SchematicContent, SchematicSymbol, SymbolPin},
};
use viamind_geom::Nm;
use viamind_render::{
    SchematicLayer, SchematicViewOptions, ViewBounds,
    schematic_labels::{LabelRole, schematic_label_layout},
    schematic_png, schematic_svg, schematic_view_snapshot,
};

fn point(x: i64, y: i64) -> Coordinate {
    Coordinate { x: Nm(x), y: Nm(y) }
}
fn symbol() -> SchematicSymbol {
    SchematicSymbol {
        id: ObjectId::new(),
        designator: "U1".into(),
        value: "10kΩ µA é Я".into(),
        footprint: None,
        position: point(17, 23),
        rotation_quarters: 0,
        mirrored: false,
        width: Nm(3),
        height: Nm(7),
        pins: [(-2, -4), (1, 3), (-1, 1), (0, 0)]
            .into_iter()
            .enumerate()
            .map(|(i, (x, y))| SymbolPin {
                id: ObjectId::new(),
                number: (i + 1).to_string(),
                name: format!("pin{i}"),
                position: point(x, y),
                electrical: PinType::Passive,
            })
            .collect(),
    }
}
fn sheet(symbol: SchematicSymbol) -> Sheet {
    let mut sheet = Sheet::new("Exact label geometry");
    let mut c = SchematicContent::default();
    c.symbols.insert(symbol.id, symbol);
    sheet.content = Some(c);
    sheet
}
fn overlap(a: ViewBounds, b: ViewBounds) -> bool {
    a.min_x < b.max_x && b.min_x < a.max_x && a.min_y < b.max_y && b.min_y < a.max_y
}
#[test]
fn independent_odd_geometry_endpoints_and_disjoint_owned_labels_all_poses() {
    for quarters in 0..4 {
        for mirrored in [false, true] {
            let mut s = symbol();
            s.rotation_quarters = quarters;
            s.mirrored = mirrored;
            let before = sheet(s.clone());
            let original = before.clone();
            let labels = schematic_label_layout(&before).unwrap();
            assert_eq!(before, original);
            for pin in &s.pins {
                let (x, y) = (if mirrored { -pin.position.x.0 } else { pin.position.x.0 }, pin.position.y.0);
                let (x, y) = match quarters {
                    0 => (x, y),
                    1 => (y, -x),
                    2 => (-x, -y),
                    _ => (-y, x),
                };
                let expected = point(17 + x, 23 + y);
                let label = labels.iter().find(|l| l.object_id == pin.id).unwrap();
                assert_eq!(label.leader.unwrap()[0], expected);
                assert_eq!(s.pin_position(pin.position).unwrap(), expected);
            }
            // Independent floor/ceil odd-width body corners before cardinal transform.
            let corners = [(-1, -3), (2, -3), (2, 4), (-1, 4)].map(|(mut x, y)| {
                if mirrored {
                    x = -x;
                }
                let (x, y) = match quarters {
                    0 => (x, y),
                    1 => (y, -x),
                    2 => (-x, -y),
                    _ => (-y, x),
                };
                (17 + x, 23 + y)
            });
            let body = ViewBounds {
                min_x: corners.iter().map(|p| p.0).min().unwrap(),
                max_x: corners.iter().map(|p| p.0).max().unwrap(),
                min_y: corners.iter().map(|p| p.1).min().unwrap(),
                max_y: corners.iter().map(|p| p.1).max().unwrap(),
            };
            for (i, label) in labels.iter().enumerate() {
                assert!(!overlap(body, label.envelope));
                for other in &labels[i + 1..] {
                    assert!(!overlap(label.envelope, other.envelope), "{label:?} {other:?}");
                }
            }
            assert_eq!(serde_json::to_value(&labels).unwrap(), serde_json::to_value(schematic_label_layout(&before).unwrap()).unwrap());
        }
    }
}

#[test]
fn dense_coincident_pins_keep_exact_identity_and_number_order_and_do_not_overlap() {
    let mut s = symbol();
    let pin = s.pins[0].clone();
    s.pins = (0..128).map(|i| SymbolPin { id: ObjectId::new(), number: i.to_string(), ..pin.clone() }).collect();
    let before = sheet(s);
    let original = before.clone();
    let labels = schematic_label_layout(&before).unwrap();
    assert_eq!(labels.len(), 130);
    assert_eq!(before, original);
    let pins: Vec<_> = labels.iter().filter(|l| l.role == LabelRole::Pin).collect();
    assert!(pins.windows(2).all(|p| p[0].object_id < p[1].object_id));
    for (i, label) in labels.iter().enumerate() {
        for other in &labels[i + 1..] {
            assert!(!overlap(label.envelope, other.envelope));
        }
    }
    assert!(pins.iter().all(|l| l.leader.unwrap()[0] == point(15, 19)));
}

#[test]
fn complete_original_package_and_export_adapters_share_exact_labels_and_preserve_project() {
    use viamind_doc::symbol_package::{self, PackageUnit, SchematicPackage};
    let t = symbol_package::from_slice(include_bytes!("../../../fixtures/symbol-packages/original-quad.viamind-symbol-package.json")).unwrap();
    let mut c = SchematicContent::default();
    let mut group = SchematicPackage { id: ObjectId::new(), template_name: t.name.clone(), units: vec![] };
    for (i, u) in t.units.iter().enumerate() {
        let s = t.unit_template(u).instantiate("U1", point(i64::try_from(i).unwrap() * 20_000_000, 0), 0, false).unwrap();
        group.units.push(PackageUnit { key: u.key.clone(), name: u.name.clone(), role: u.role, symbol_id: s.id });
        c.symbols.insert(s.id, s);
    }
    c.packages.insert(group.id, group);
    let mut sh = Sheet::new("Original five units");
    sh.content = Some(c);
    let labels = schematic_label_layout(&sh).unwrap();
    assert_eq!(labels.len(), 24);
    for key in ["A", "B", "C", "D", "P"] {
        assert!(labels.iter().any(|l| l.role == LabelRole::Reference && l.text.starts_with(&format!("U1:{key} ("))));
    }
    let mut p = Project::new("Original view");
    p.schematic.sheets.insert(sh.id, sh.clone());
    let before = p.clone();
    let snap = schematic_view_snapshot(&p, sh.id).unwrap();
    let options = SchematicViewOptions::default();
    let svg = schematic_svg(&snap, &options).unwrap();
    let png = schematic_png(&snap, &options).unwrap();
    assert_eq!(serde_json::to_value(&svg.metadata.symbol_labels).unwrap(), serde_json::to_value(&labels).unwrap());
    assert_eq!(serde_json::to_value(&png.metadata.symbol_labels).unwrap(), serde_json::to_value(&labels).unwrap());
    assert!(svg.svg.contains("pin-leader-"));
    assert_eq!(p, before);
}

#[test]
fn source_bounds_precede_malformed_geometry_and_abbreviation_does_not_hide_unsupported_glyphs() {
    let mut s = symbol();
    s.width = Nm(0);
    s.value = "x".repeat(4097);
    assert!(schematic_label_layout(&sheet(s)).unwrap_err().to_string().contains("4096"));
    let mut sh = Sheet::new("Raw structural poison");
    let mut c = SchematicContent::default();
    for _ in 0..10_001 {
        let mut s = symbol();
        s.width = Nm(0);
        c.symbols.insert(s.id, s);
    }
    sh.content = Some(c);
    assert!(schematic_label_layout(&sh).unwrap_err().to_string().contains("10000"));
    let mut s = symbol();
    s.value = "A".repeat(33) + "漢";
    let sh = sheet(s);
    let labels = schematic_label_layout(&sh).unwrap();
    let value = labels.iter().find(|l| l.role == LabelRole::Value).unwrap();
    assert!(value.abbreviated);
    assert_eq!(value.display_text, "A".repeat(32) + "...");
    assert!(value.text.ends_with('漢'));
    let mut p = Project::new("Glyph admission");
    p.schematic.sheets.insert(sh.id, sh.clone());
    let snap = schematic_view_snapshot(&p, sh.id).unwrap();
    assert!(schematic_svg(&snap, &Default::default()).unwrap_err().to_string().contains("U+6F22"));
    assert!(schematic_svg(&snap, &SchematicViewOptions { layers: vec![SchematicLayer::Wires], ..Default::default() }).is_ok());
}

#[test]
fn generated_ellipsis_suffixes_are_charged_before_display_strings() {
    let mut sh = Sheet::new("Display suffix budget");
    let mut c = SchematicContent::default();
    for i in 0..850 {
        let mut s = symbol();
        s.designator = format!("R{i}");
        s.value = "A".repeat(33);
        s.pins.clear();
        c.symbols.insert(s.id, s);
    }
    sh.content = Some(c);
    assert!(schematic_label_layout(&sh).unwrap_err().to_string().contains("display exceeds 32768"));
}

#[test]
fn selected_symbol_and_pin_layers_isolate_text_leaders_and_full_source_glyph_admission() {
    let mut s = symbol();
    s.pins[0].name = "漢".into();
    let sh = sheet(s);
    let mut p = Project::new("Selected layers");
    p.schematic.sheets.insert(sh.id, sh.clone());
    let snap = schematic_view_snapshot(&p, sh.id).unwrap();
    let symbols = schematic_svg(&snap, &SchematicViewOptions { layers: vec![SchematicLayer::Symbols], ..Default::default() }).unwrap();
    assert!(symbols.svg.contains("reference-") && symbols.svg.contains("value-"));
    assert!(!symbols.svg.contains("pin-text-") && !symbols.svg.contains("pin-leader-") && !symbols.svg.contains("pin-ring-"));
    assert!(symbols.metadata.symbol_labels.iter().all(|l| l.role != LabelRole::Pin));
    assert!(
        schematic_svg(&snap, &SchematicViewOptions { layers: vec![SchematicLayer::Pins], ..Default::default() })
            .unwrap_err()
            .to_string()
            .contains("U+6F22")
    );
    let mut s = symbol();
    s.value = "漢".into();
    let sh = sheet(s);
    p.schematic.sheets.clear();
    p.schematic.sheets.insert(sh.id, sh.clone());
    let snap = schematic_view_snapshot(&p, sh.id).unwrap();
    let pins = schematic_svg(&snap, &SchematicViewOptions { layers: vec![SchematicLayer::Pins], ..Default::default() }).unwrap();
    assert!(pins.svg.contains("pin-text-") && pins.svg.contains("pin-leader-"));
    assert!(!pins.svg.contains("reference-") && !pins.svg.contains("value-") && !pins.svg.contains("symbol-body-"));
    assert!(pins.metadata.symbol_labels.iter().all(|l| l.role == LabelRole::Pin));
    let empty = schematic_svg(&snap, &SchematicViewOptions { layers: vec![], ..Default::default() }).unwrap_err().to_string();
    assert!(empty.contains("layers requires") && !empty.contains("U+6F22"));
}
