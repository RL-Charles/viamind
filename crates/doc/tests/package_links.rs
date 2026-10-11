use viamind_doc::{ObjectId, Part, Project, Side, file};
use viamind_geom::Point;

fn fixture() -> (Project, ObjectId) {
    let mut project = file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
    let package = ObjectId::new();
    let part = Part {
        artwork: None,
        assembly: None,
        schematic_symbol: None,
        schematic_package: Some(package),
        pads: None,
        id: ObjectId::new(),
        designator: "PCB1".into(),
        value: "authored".into(),
        footprint: "explicit".into(),
        position: Point::from_mm(4.0, 8.0),
        rotation_mdeg: 90_000,
        side: Side::Bottom,
    };
    project.board.parts.insert(part.id, part);
    (project, package)
}

#[test]
fn unresolved_package_identity_is_preserved_at_twelve_and_absence_preserves_old_bytes() {
    let (mut project, package) = fixture();
    let text = file::to_string(&project);
    assert_eq!(file::minimum_version(&project), 12);
    assert_eq!(file::from_str(&text).unwrap(), project);
    assert!(file::from_str(&text.replace("\"version\": 12", "\"version\": 11")).is_err());
    let part = project.board.parts.values().next().unwrap();
    assert_eq!(part.clone_without_assembly().schematic_package, Some(package));
    project.board.parts.values_mut().next().unwrap().schematic_package = None;
    let old = file::to_string(&project);
    assert_eq!(file::minimum_version(&project), 5);
    assert!(!old.contains("schematic_package"));
    assert_eq!(file::from_str(&old).unwrap(), project);
    // Canonical bytes produced by retained pre-package-link production source3ec6ccb,
    // not the authored input fixture's intentionally independent field order.
    let fixture = include_str!("../../../fixtures/schematic/linked-regulator-canonical.viamind.json");
    assert_eq!(file::to_string(&file::from_str(fixture).unwrap()), fixture, "old symbol-link native bytes stay exact");
}

#[test]
fn dual_nil_duplicate_and_wrong_kind_package_links_refuse() {
    let (mut project, package) = fixture();
    let part_id = *project.board.parts.keys().next().unwrap();
    let symbol = *project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().symbols.keys().next().unwrap();
    project.board.parts.get_mut(&part_id).unwrap().schematic_symbol = Some(symbol);
    assert!(file::from_str(&file::to_string(&project)).unwrap_err().to_string().contains("never both"));
    project.board.parts.get_mut(&part_id).unwrap().schematic_symbol = None;
    for invalid in [ObjectId::from_uuid(uuid::Uuid::nil()), project.id, part_id, symbol, *project.schematic.sheets.keys().next().unwrap()] {
        project.board.parts.get_mut(&part_id).unwrap().schematic_package = Some(invalid);
        assert!(file::from_str(&file::to_string(&project)).is_err());
    }
    project.board.parts.get_mut(&part_id).unwrap().schematic_package = Some(package);
    let mut duplicate = project.board.parts[&part_id].clone();
    duplicate.id = ObjectId::new();
    duplicate.designator = "PCB2".into();
    project.board.parts.insert(duplicate.id, duplicate);
    assert!(file::from_str(&file::to_string(&project)).unwrap_err().to_string().contains("multiple PCB"));
}
