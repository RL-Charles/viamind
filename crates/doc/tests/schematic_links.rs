use viamind_doc::{ObjectId, Part, Project, Side, file};
use viamind_geom::Point;

fn fixture() -> (Project, ObjectId) {
    let mut project = file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap();
    let symbol = *project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().symbols.keys().next().unwrap();
    let part = Part {
        artwork: None,
        assembly: None,
        schematic_symbol: Some(symbol),
        schematic_package: None,
        pads: None,
        id: ObjectId::new(),
        designator: "PCB1".into(),
        value: "original".into(),
        footprint: "explicit".into(),
        position: Point::from_mm(4.0, 8.0),
        rotation_mdeg: 90_000,
        side: Side::Bottom,
    };
    project.board.parts.insert(part.id, part);
    (project, symbol)
}

#[test]
fn native_links_survive_reference_changes_and_require_schema_six() {
    let (mut project, symbol) = fixture();
    let text = file::to_string(&project);
    assert!(text.contains("\"version\": 6"));
    assert_eq!(file::from_str(&text).unwrap(), project);
    assert!(file::from_str(&text.replace("\"version\": 6", "\"version\": 5")).is_err());
    project.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.get_mut(&symbol).unwrap().designator = "Z999".into();
    assert_eq!(file::from_str(&file::to_string(&project)).unwrap(), project);
    assert_eq!(project.board.parts.values().next().unwrap().schematic_symbol, Some(symbol));
    project.board.parts.values_mut().next().unwrap().schematic_symbol = None;
    let legacy = file::to_string(&project);
    assert!(legacy.contains("\"version\": 5"));
    assert!(!legacy.contains("schematic_symbol"));
    assert_eq!(file::from_str(&legacy).unwrap(), project);
}

#[test]
fn duplicate_nil_and_wrong_kind_links_reject_but_missing_symbols_remain_unresolved() {
    let (mut project, symbol) = fixture();
    let original = project.board.parts.values().next().unwrap().clone();
    let mut duplicate = original.clone();
    duplicate.id = ObjectId::new();
    project.board.parts.insert(duplicate.id, duplicate.clone());
    assert!(file::from_str(&file::to_string(&project)).unwrap_err().to_string().contains("multiply linked"));
    project.board.parts.remove(&duplicate.id);
    for invalid in [ObjectId::from_uuid(uuid::Uuid::nil()), project.id, original.id, *project.schematic.sheets.keys().next().unwrap()] {
        project.board.parts.values_mut().next().unwrap().schematic_symbol = Some(invalid);
        assert!(file::from_str(&file::to_string(&project)).is_err());
    }
    project.board.parts.values_mut().next().unwrap().schematic_symbol = Some(symbol);
    project.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap().symbols.remove(&symbol);
    assert_eq!(file::from_str(&file::to_string(&project)).unwrap(), project, "deletion preserves the unresolved identity instead of relinking");
}
