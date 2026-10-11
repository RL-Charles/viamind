use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{ConvertFootprintFile, EngineHandle, FootprintFile, Origin, PreviewFootprintConversion};
const SOURCE: &str = include_str!("../../../fixtures/import/original-rect-smd.kicad_mod");
#[test]
fn detached_conversion_preserves_dirty_session_and_reuse_preserves_numbered_identity_and_nets() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_mod");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("keep"));
    let mut ids = Vec::new();
    for reference in ["U1", "U2"] {
        let id = engine
            .execute("add_part", json!({"designator":reference,"value":"keep","footprint":"Old","x_mm":1.000003,"y_mm":2.000007}), Origin::User)
            .unwrap()
            .created[0];
        let part = engine.read(|p| p.board.part(&id).unwrap().clone());
        engine
            .execute(
                "set_part_pads",
                json!({"id":id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":"VIN"}]}),
                Origin::User,
            )
            .unwrap();
        ids.push(id);
    }
    engine.set_selection(viamind_engine::SetSelection { ids: ids.clone(), mode: Default::default() }, Origin::User).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    assert!(status.dirty);
    let preview = engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).unwrap();
    let second = engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).unwrap();
    assert_eq!(serde_json::to_value(&preview).unwrap(), serde_json::to_value(second).unwrap());
    assert!(preview.can_convert && !preview.report.full_fidelity);
    let output = directory.path().join("converted.viamind-footprint.json");
    let result = engine
        .convert_footprint_file(ConvertFootprintFile {
            path: source.clone(),
            expected_source_sha256: preview.source_sha256,
            destination: output.clone(),
        })
        .unwrap();
    assert_eq!(result.footprint, viamind_doc::footprint::load(&output).unwrap());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let mut new_pad_ids = Vec::new();
    for id in ids {
        let expected = engine.read(|p| p.board.part(&id).unwrap().clone());
        let old_pad = &expected.pads.as_ref().unwrap().0[0];
        engine
            .import_footprint(FootprintFile { expected_project: before.project.id, expected: expected.clone(), path: output.clone() }, Origin::User)
            .unwrap();
        let changed = engine.read(|p| p.board.part(&id).unwrap().clone());
        let pads = &changed.pads.as_ref().unwrap().0;
        assert_eq!(pads[0].id, old_pad.id);
        assert_eq!(pads[0].net, old_pad.net);
        assert!(pads[1].net.is_none());
        assert_ne!(pads[1].id, old_pad.id);
        new_pad_ids.push(pads[1].id);
        assert_eq!(changed.position, expected.position);
        assert_eq!(pads[0].position.x.0, -500003);
        assert_eq!(pads[1].position.y.0, -125003);
    }
    assert_ne!(new_pad_ids[0], new_pad_ids[1]);
    engine.undo(Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, before.project);
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
}
#[test]
fn changed_blocked_sources_conflicts_native_aliases_and_bounded_io_preserve_all_bytes_and_session() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_mod");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("keep"));
    let native = directory.path().join("native.viamind.json");
    engine.save_project(viamind_engine::SaveProject { path: Some(native.clone()), overwrite: false }).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let native_bytes = std::fs::read(&native).unwrap();
    let preview = engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).unwrap();
    let output = directory.path().join("new.json");
    let command = ConvertFootprintFile { path: source.clone(), expected_source_sha256: preview.source_sha256, destination: output.clone() };
    let mut invalid = command.clone();
    invalid.expected_source_sha256 = "wrong".into();
    assert!(engine.convert_footprint_file(invalid).is_err());
    std::fs::write(&source, format!("{SOURCE}\n")).unwrap();
    assert!(engine.convert_footprint_file(command.clone()).is_err());
    assert!(!output.exists());
    std::fs::write(&source, SOURCE.replace("smd rect", "smd circle")).unwrap();
    let blocked = engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).unwrap();
    assert!(!blocked.can_convert && blocked.footprint.is_none());
    assert!(engine.convert_footprint_file(ConvertFootprintFile { expected_source_sha256: blocked.source_sha256, ..command.clone() }).is_err());
    std::fs::write(&source, SOURCE).unwrap();
    for destination in [&source, &native] {
        assert!(engine.convert_footprint_file(ConvertFootprintFile { destination: destination.clone(), ..command.clone() }).is_err());
    }
    std::fs::write(&output, b"keep").unwrap();
    assert!(engine.convert_footprint_file(command.clone()).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"keep");
    #[cfg(unix)]
    {
        let alias = directory.path().join("alias.json");
        std::os::unix::fs::symlink(&native, &alias).unwrap();
        assert!(engine.convert_footprint_file(ConvertFootprintFile { destination: alias, ..command.clone() }).is_err());
    }
    for path in [directory.path().to_owned(), std::path::PathBuf::from("x".repeat(4097))] {
        assert!(engine.preview_footprint_conversion(PreviewFootprintConversion { path }).is_err());
    }
    std::fs::write(&source, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).is_err());
    std::fs::write(&source, [255]).unwrap();
    assert!(engine.preview_footprint_conversion(PreviewFootprintConversion { path: source.clone() }).is_err());
    std::fs::write(&source, SOURCE).unwrap();
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read(native).unwrap(), native_bytes);
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
}
