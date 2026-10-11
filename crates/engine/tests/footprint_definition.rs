use serde_json::json;
use viamind_doc::{
    Project,
    footprint::{Footprint, FootprintPad, LocalPosition},
};
use viamind_engine::{EngineHandle, FootprintFile, LoadFootprintDefinition, Origin, SaveFootprintDefinition};
use viamind_geom::Nm;

fn original() -> Footprint {
    Footprint {
        name: "Original asymmetric definition".into(),
        pads: vec![
            FootprintPad { number: "1".into(), position: LocalPosition { x: Nm(-500003), y: Nm(250001) }, width: Nm(1200003), height: Nm(800001) },
            FootprintPad { number: "2".into(), position: LocalPosition { x: Nm(750007), y: Nm(-125003) }, width: Nm(600003), height: Nm(1000001) },
        ],
    }
}
#[test]
fn detached_files_preserve_dirty_session_and_separate_reuse_retains_link_assembly_ids_nets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.json");
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/linked-regulator.viamind.json")).unwrap();
    project.board.parts.values_mut().next().unwrap().assembly =
        Some(viamind_doc::AssemblyMetadata { mpn: Some("Explicit retained MPN".into()), populate: false, ..Default::default() });
    let engine = EngineHandle::new(project);
    let id = engine.read(|p| *p.board.parts.keys().next().unwrap());
    let old = engine.read(|p| p.board.part(&id).unwrap().clone());
    assert!(old.schematic_symbol.is_some());
    engine.execute("update_part", json!({"id":id,"expected":old,"value":"dirty keep"}), Origin::User).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let expected = before.project.board.part(&id).unwrap().clone();
    assert!(status.dirty);
    let mut definition = original();
    // Match one existing number and add one genuinely new number.
    definition.pads[0].number = expected.pads.as_ref().unwrap().0[0].number.clone();
    definition.pads[1].number = "NEW".into();
    let saved = engine.save_footprint_definition(SaveFootprintDefinition { footprint: definition.clone(), path: path.clone() }).unwrap();
    assert_eq!(saved.pad_count, 2);
    assert_eq!(engine.load_footprint_definition(LoadFootprintDefinition { path: saved.path }).unwrap(), definition);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    engine.import_footprint(FootprintFile { expected_project: before.project.id, expected: expected.clone(), path }, Origin::User).unwrap();
    let changed = engine.read(|p| p.board.part(&id).unwrap().clone());
    assert_eq!(changed.position, expected.position);
    assert_eq!(changed.rotation_mdeg, expected.rotation_mdeg);
    assert_eq!(changed.side, expected.side);
    assert_eq!(changed.schematic_symbol, expected.schematic_symbol);
    assert_eq!(changed.assembly, expected.assembly);
    let pads = &changed.pads.as_ref().unwrap().0;
    let previous = &expected.pads.as_ref().unwrap().0[0];
    assert_eq!(pads[0].id, previous.id);
    assert_eq!(pads[0].net, previous.net);
    assert_ne!(pads[1].id, previous.id);
    assert!(pads[1].net.is_none());
    assert_eq!((pads[0].position.x.0, pads[0].width.0, pads[0].height.0), (-500003, 1200003, 800001));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, before.project);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.part(&id).unwrap().clone()), changed);
}
#[test]
fn invalid_bounds_precision_free_integer_geometry_and_existing_files_reject_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.json");
    let engine = EngineHandle::new(Project::new("keep"));
    let before = engine.session_state();
    for mutation in 0..6 {
        let mut footprint = original();
        match mutation {
            0 => footprint.name = "x".repeat(513),
            1 => footprint.pads = vec![footprint.pads[0].clone(); 1025],
            2 => footprint.pads[1].number = footprint.pads[0].number.clone(),
            3 => footprint.pads[0].width = Nm(0),
            4 => footprint.pads[0].position.x = Nm(i64::MIN),
            _ => footprint.pads[0].number = " control\n".into(),
        }
        assert!(engine.save_footprint_definition(SaveFootprintDefinition { footprint, path: path.clone() }).is_err());
        assert!(!path.exists());
    }
    std::fs::write(&path, b"keep").unwrap();
    assert!(engine.save_footprint_definition(SaveFootprintDefinition { footprint: original(), path: path.clone() }).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    assert!(engine.load_footprint_definition(LoadFootprintDefinition { path: path.clone() }).is_err());
    #[cfg(unix)]
    {
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(engine.save_footprint_definition(SaveFootprintDefinition { footprint: original(), path: alias }).is_err());
    }
    assert!(engine.load_footprint_definition(LoadFootprintDefinition { path: "x".repeat(4097).into() }).is_err());
    let empty = directory.path().join("empty.json");
    engine
        .save_footprint_definition(SaveFootprintDefinition {
            footprint: Footprint { name: "Explicit empty".into(), pads: vec![] },
            path: empty.clone(),
        })
        .unwrap();
    assert!(engine.load_footprint_definition(LoadFootprintDefinition { path: empty }).unwrap().pads.is_empty());
    assert_eq!(engine.session_state(), before);
    assert!(!engine.document_status().dirty);
}
