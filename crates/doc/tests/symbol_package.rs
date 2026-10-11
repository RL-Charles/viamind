use serde_json::{Value, json};
use viamind_doc::{
    ObjectId, Project, Sheet, file,
    schematic::{Coordinate, PinType, SchematicContent},
    symbol_library::TemplatePin,
    symbol_package::{self as package, PackageUnit, SchematicPackage, SymbolPackageTemplate, UnitRole, UnitTemplate},
};
use viamind_geom::Nm;

fn definition() -> SymbolPackageTemplate {
    let mut units = vec![];
    for i in 0..5 {
        let power = i == 4;
        units.push(UnitTemplate {
            key: if power { "P".into() } else { char::from(b'A' + i).to_string() },
            name: if power { "Power".into() } else { format!("Amplifier {}", i + 1) },
            role: if power { UnitRole::Power } else { UnitRole::Functional },
            width: Nm(6_000_003),
            height: Nm(4_000_007),
            pins: (0..if power { 2 } else { 3 })
                .map(|j| TemplatePin {
                    number: (usize::from(i) * 3 + j + 1).to_string(),
                    name: if power { ["V+", "V-"][j].into() } else { ["IN+", "IN-", "OUT"][j].into() },
                    position: Coordinate { x: Nm(if j == 2 { 3_000_003 } else { -3_000_003 }), y: Nm((j as i64 - 1) * 1_000_007) },
                    electrical: if power {
                        PinType::PowerInput
                    } else if j == 2 {
                        PinType::Output
                    } else {
                        PinType::Input
                    },
                })
                .collect(),
        });
    }
    SymbolPackageTemplate { name: "Original four amplifiers plus power".into(), value: "Authored fourteen pins".into(), footprint: None, units }
}
fn project() -> (Project, ObjectId, ObjectId) {
    let t = definition();
    let mut p = Project::new("Explicit package");
    let mut sheet = Sheet::new("Units");
    let mut c = SchematicContent::default();
    let id = ObjectId::new();
    let mut units = vec![];
    for (i, u) in t.units.iter().enumerate() {
        let s = t.unit_template(u).instantiate("U1", Coordinate { x: Nm(i as i64 * 10_000_000), y: Nm(7) }, (i % 4) as u8, i == 4).unwrap();
        units.push(PackageUnit { key: u.key.clone(), name: u.name.clone(), role: u.role, symbol_id: s.id });
        c.symbols.insert(s.id, s);
    }
    c.packages.insert(id, SchematicPackage { id, template_name: t.name, units });
    sheet.content = Some(c);
    let sid = sheet.id;
    p.schematic.sheets.insert(sid, sheet);
    (p, sid, id)
}
#[test]
fn original_five_unit_definition_and_native_package_have_exact_fourteen_pin_bijection() {
    let t = definition();
    assert_eq!(package::from_slice(&package::to_vec(&t).unwrap()).unwrap(), t);
    let (p, sid, id) = project();
    viamind_doc::schematic::validate_project(&p, None).unwrap();
    let c = p.schematic.sheets[&sid].content.as_ref().unwrap();
    let map = package::package_pin_map(c, &c.packages[&id]).unwrap();
    assert_eq!(map.keys().cloned().collect::<std::collections::BTreeSet<_>>(), (1..=14).map(|i| i.to_string()).collect());
    assert_eq!(map["13"].0, "P");
    assert_eq!(map["3"].0, "A");
    assert_eq!(c.symbols.len(), 5);
    assert!(c.symbols.values().all(|s| s.designator == "U1"));
    assert_eq!(file::minimum_version(&p), 11);
    let bytes = file::to_string(&p);
    assert_eq!(file::from_str(&bytes).unwrap(), p);
    assert!(file::from_str(&bytes.replacen("\"version\": 11", "\"version\": 10", 1)).is_err());
    let empty = Project::new("Legacy");
    let bytes = file::to_string(&empty);
    assert!(!bytes.contains("packages"));
    assert_eq!(file::minimum_version(&empty), 1);
}
#[test]
fn incomplete_ambiguous_pin_reference_and_identity_membership_rejects() {
    for case in 0..7 {
        let (mut p, sid, id) = project();
        let c = p.schematic.sheets.get_mut(&sid).unwrap().content.as_mut().unwrap();
        let unit = c.packages[&id].units[0].symbol_id;
        match case {
            0 => {
                c.symbols.remove(&unit);
            }
            1 => {
                c.symbols.get_mut(&unit).unwrap().pins[0].number = "13".into();
            }
            2 => {
                c.symbols.get_mut(&unit).unwrap().value = "inconsistent".into();
            }
            3 => {
                let mut other = c.packages[&id].clone();
                other.id = ObjectId::new();
                c.packages.insert(other.id, other);
            }
            4 => {
                let mut other = c.symbols[&unit].clone();
                other.id = ObjectId::new();
                other.pins.clear();
                c.symbols.insert(other.id, other);
            }
            5 => {
                let mut group = c.packages.remove(&id).unwrap();
                group.id = c.symbols[&unit].pins[0].id;
                c.packages.insert(group.id, group);
            }
            _ => {
                c.packages.get_mut(&id).unwrap().units[0].key = "P".into();
            }
        }
        assert!(viamind_doc::schematic::validate_project(&p, None).is_err(), "case {case}");
    }
}
#[test]
fn borrowed_counts_precede_excess_malformed_units_or_pins_and_text_limits() {
    let mut v = serde_json::to_value(definition()).unwrap();
    v["units"] = Value::Array(vec![Value::Null; 33]);
    assert!(serde_json::from_value::<SymbolPackageTemplate>(v).unwrap_err().to_string().contains("2..32"));
    let mut v = serde_json::to_value(definition()).unwrap();
    v["units"][0]["pins"] = Value::Array(vec![Value::Null; 1025]);
    assert!(serde_json::from_value::<SymbolPackageTemplate>(v).unwrap_err().to_string().contains("1024"));
    let mut v = serde_json::to_value(definition()).unwrap();
    v["units"][0]["name"] = json!("x".repeat(4097));
    assert!(package::predecode(&v).unwrap_err().contains("4096"));
    let mut v = json!({"name":"","value":"","footprint":null,"units":[{"key":"A","name":"","pins":[]},{"key":"B","name":"","pins":[]}]});
    for i in 0..2 {
        v["units"][i]["pins"] = json!(vec![json!({"number":"x".repeat(128),"name":"y".repeat(128)}); 512]);
    }
    // Exact aggregate minus the two explicit key bytes is admitted before semantic decoding.
    v["units"][0]["pins"][0]["name"] = json!("y".repeat(126));
    assert!(package::predecode(&v).is_ok());
    v["name"] = json!("x");
    assert!(package::predecode(&v).unwrap_err().contains("256 KiB"));
    let id = ObjectId::new();
    let mut g = json!({"id":id,"template_name":"Test","units":[]});
    g["units"] = Value::Array(vec![Value::Null; 33]);
    assert!(serde_json::from_value::<SchematicPackage>(g).unwrap_err().to_string().contains("32"));
}
#[test]
fn strict_raw_duplicate_keys_and_nested_semantics_refuse() {
    let bytes = String::from_utf8(package::to_vec(&definition()).unwrap()).unwrap();
    assert!(package::from_slice(bytes.replacen("\"version\": 1", "\"version\": 1, \"version\": 1", 1).as_bytes()).unwrap_err().contains("duplicate"));
    assert!(package::from_slice(bytes.replacen("\"key\": \"A\"", "\"key\": \"A\", \"key\": \"B\"", 1).as_bytes()).unwrap_err().contains("duplicate"));
    let mut v: Value = serde_json::from_str(&bytes).unwrap();
    v["package"]["units"][0]["semantic_extension"] = json!({"implicit_power":true});
    assert!(package::from_slice(&serde_json::to_vec(&v).unwrap()).is_err());
    assert!(package::from_slice(&vec![b' '; package::MAX_BYTES + 1]).unwrap_err().contains("1 MiB"));
}
#[test]
fn synchronized_new_package_files_preserve_native_aliases_and_racing_destinations() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("original.viamind-symbol-package.json");
    let t = definition();
    let staged = package::stage_new(&t, &path).unwrap();
    assert!(!path.exists());
    drop(staged);
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    assert!(package::stage_new(&t, &path).unwrap().publish(Some(&path)).unwrap_err().contains("active native"));
    let staged = package::stage_new(&t, &path).unwrap();
    std::fs::write(&path, b"racing bytes").unwrap();
    assert!(staged.publish(None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"racing bytes");
    std::fs::remove_file(&path).unwrap();
    package::save_new(&t, &path, None).unwrap();
    assert_eq!(package::load(&path).unwrap(), t);
    assert!(package::save_new(&t, &path, None).is_err());
    #[cfg(unix)]
    {
        let fifo = d.path().join("raced-fifo");
        rustix::fs::mkfifoat(rustix::fs::CWD, &fifo, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR).unwrap();
        assert!(package::load(&fifo).unwrap_err().contains("regular file"));
        let alias = d.path().join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(package::stage_new(&t, &alias).unwrap().publish(Some(&path)).unwrap_err().contains("active native"));
    }
    assert!(package::load(std::path::Path::new(&"x".repeat(4097))).unwrap_err().contains("4096"));
}

#[test]
fn odd_nm_body_corners_and_transformed_pins_have_independent_exact_boundary_oracles() {
    let mut unit = definition().units.remove(0);
    unit.width = Nm(3);
    unit.height = Nm(7);
    unit.pins.clear();
    for quarters in 0..4 {
        for mirrored in [false, true] {
            // An odd centered width needs ceil(width/2) clearance at an integer
            // center. Swapping axes changes that ceiling independently of pins.
            let half_up = if quarters % 2 == 0 { 2 } else { 4 };
            let admitted = Coordinate { x: Nm(10_000_000_000 - half_up), y: Nm(0) };
            assert!(SymbolPackageTemplate::validate_unit_pose(&unit, admitted, quarters, mirrored).is_ok());
            let refused = Coordinate { x: Nm(admitted.x.0 + 1), y: Nm(0) };
            assert!(SymbolPackageTemplate::validate_unit_pose(&unit, refused, quarters, mirrored).unwrap_err().contains("doubled-Nm"));
        }
    }
    unit.pins.push(TemplatePin {
        number: "1".into(),
        name: String::new(),
        position: Coordinate { x: Nm(-10_000_000_000), y: Nm(0) },
        electrical: PinType::Input,
    });
    assert!(
        SymbolPackageTemplate::validate_unit_pose(&unit, Coordinate { x: Nm(1), y: Nm(0) }, 0, true).is_err(),
        "mirrored endpoint exceeds positive bound"
    );
    let (mut p, sid, id) = project();
    let c = p.schematic.sheets.get_mut(&sid).unwrap().content.as_mut().unwrap();
    let symbol = c.packages[&id].units[0].symbol_id;
    let s = c.symbols.get_mut(&symbol).unwrap();
    s.position.x = Nm(10_000_000_000);
    s.pins.clear();
    // Preserve a nonempty locally valid pin so the body-only failure is isolated.
    s.pins.push(viamind_doc::schematic::SymbolPin {
        id: ObjectId::new(),
        number: "1".into(),
        name: String::new(),
        position: Coordinate { x: Nm(0), y: Nm(0) },
        electrical: PinType::Passive,
    });
    assert!(viamind_doc::schematic::validate_project(&p, None).unwrap_err().contains("body"));
}
