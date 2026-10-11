use serde_json::json;
use viamind_doc::{ObjectId, Project};
use viamind_engine::{EngineHandle, ImportPcb, NewProject, OpenProject, Origin, PreviewPcbImport, SaveProject};
const SOURCE: &str = include_str!("../../../fixtures/import/original-two-layer.kicad_pcb");
fn input(p: &viamind_engine::PcbImportPreview) -> ImportPcb {
    ImportPcb {
        path: p.path.clone(),
        expected_project: p.expected_project,
        expected_cursor: p.expected_cursor,
        expected_source_sha256: p.source_sha256.clone(),
    }
}
#[test]
fn preview_is_read_only_import_is_unsaved_and_native_roundtrip_retains_exact_geometry() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_pcb");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("existing"));
    let original = directory.path().join("existing.viamind.json");
    engine.save_project(SaveProject { path: Some(original.clone()), overwrite: false }).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let preview = engine.preview_pcb_import(PreviewPcbImport { path: source.clone() }).unwrap();
    assert!(preview.can_import);
    assert_eq!(engine.preview_pcb_import(PreviewPcbImport { path: source.clone() }).unwrap().report, preview.report);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let result = engine.import_pcb(input(&preview), Origin::User).unwrap();
    assert!(result.document.dirty);
    assert!(result.document.path.is_none());
    assert_ne!(result.cursor.stream_id, before.cursor.stream_id);
    assert!(engine.session_state().selection.ids.is_empty());
    assert!(!engine.can_undo());
    assert!(!engine.can_redo());
    let imported = engine.session_state().project;
    let id = *imported.board.parts.keys().next().unwrap();
    engine.execute("update_part", json!({"id":id,"designator":"U2"}), Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, imported);
    assert!(engine.document_status().dirty, "undo must not silently make an unsaved conversion disposable");
    assert!(engine.new_project(NewProject::default(), Origin::User).is_err());
    assert!(engine.save_project(SaveProject::default()).is_err(), "source never becomes active native save destination");
    let native = directory.path().join("imported.viamind.json");
    engine.save_project(SaveProject { path: Some(native.clone()), overwrite: false }).unwrap();
    assert!(!engine.document_status().dirty);
    engine.open_project(OpenProject { path: native, discard_dirty: false, expected_cursor: Some(engine.event_cursor()) }, Origin::User).unwrap();
    assert_eq!(engine.session_state().project, imported);
    assert!(!engine.document_status().dirty);
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    assert_eq!(viamind_doc::file::load(&original).unwrap(), before.project);
}
#[test]
fn stale_source_project_cursor_dirty_and_unsupported_imports_preserve_everything() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.kicad_pcb");
    std::fs::write(&source, SOURCE).unwrap();
    let engine = EngineHandle::new(Project::new("existing"));
    let p = engine.preview_pcb_import(PreviewPcbImport { path: source.clone() }).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let mut request = input(&p);
    request.expected_project = ObjectId::new();
    assert!(engine.import_pcb(request, Origin::User).is_err());
    request = input(&p);
    request.expected_cursor.sequence += 1;
    assert!(engine.import_pcb(request, Origin::User).is_err());
    request = input(&p);
    request.expected_source_sha256 = "x".repeat(64);
    assert!(engine.import_pcb(request, Origin::User).is_err());
    std::fs::write(&source, format!("{SOURCE}\n")).unwrap();
    assert!(engine.import_pcb(input(&p), Origin::User).is_err());
    std::fs::write(&source, SOURCE.replace("smd rect", "smd roundrect")).unwrap();
    let blocked = engine.preview_pcb_import(PreviewPcbImport { path: source.clone() }).unwrap();
    assert!(!blocked.can_import);
    assert!(engine.import_pcb(input(&blocked), Origin::User).is_err());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    std::fs::write(&source, SOURCE).unwrap();
    engine.execute("add_part", json!({"designator":"KEEP1","value":"unsaved","footprint":"test","x_mm":0,"y_mm":0}), Origin::User).unwrap();
    let dirty = engine.session_state();
    let dirty_status = engine.document_status();
    let fresh = engine.preview_pcb_import(PreviewPcbImport { path: source.clone() }).unwrap();
    assert!(engine.import_pcb(input(&fresh), Origin::User).is_err());
    assert_eq!(engine.session_state(), dirty);
    assert_eq!(engine.document_status(), dirty_status);
    engine.undo(Origin::User).unwrap();
    let saved = engine.session_state();
    assert!(!engine.document_status().dirty);
    engine.new_project(NewProject { name: "replacement".into(), ..Default::default() }, Origin::User).unwrap();
    let replacement = engine.session_state();
    assert_ne!(replacement.cursor.stream_id, saved.cursor.stream_id);
    assert!(engine.import_pcb(input(&p), Origin::User).is_err());
    assert_eq!(engine.session_state(), replacement);
}
#[test]
fn nonregular_long_path_invalid_utf8_and_large_sources_reject_read_only() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source");
    let engine = EngineHandle::new(Project::new("keep"));
    let before = engine.session_state();
    for bytes in [vec![0xff], vec![b'x'; viamind_doc::pcb_sexpr::MAX_SOURCE_BYTES + 1]] {
        std::fs::write(&path, bytes).unwrap();
        assert!(engine.preview_pcb_import(PreviewPcbImport { path: path.clone() }).is_err());
    }
    assert!(engine.preview_pcb_import(PreviewPcbImport { path: directory.path().into() }).is_err());
    assert!(engine.preview_pcb_import(PreviewPcbImport { path: "x".repeat(4097).into() }).is_err());
    #[cfg(target_os = "linux")]
    {
        let fifo = directory.path().join("fifo");
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        assert!(engine.preview_pcb_import(PreviewPcbImport { path: fifo }).is_err());
    }
    assert_eq!(engine.session_state(), before);
}
