use viamind_doc::{Project, fabrication_view::FabricationPrimitive};
use viamind_engine::{EngineHandle, InspectFabricationFiles, Origin};
use viamind_geom::{Nm, Point};
static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn source(id: viamind_doc::ObjectId) -> String {
    format!(
        "%FSLAX56Y56*%%MOMM*%%TF.FileFunction,Copper,L1,Top*%%TF.FilePolarity,Positive*%%TF.SameCoordinates,{id}*%%ADD10C,0.600001*%%LPD*%%LMN*%%LS1*%G01*D10*X1000001Y-2000003D03*M02*"
    )
}
fn input(e: &EngineHandle, path: std::path::PathBuf) -> InspectFabricationFiles {
    InspectFabricationFiles { paths: vec![path], expected_project: e.read(|p| p.id), expected_cursor: e.event_cursor() }
}
#[test]
fn read_only_exact_file_content_hash_and_stale_input_guards_preserve_session() {
    let _guard = TEST.lock().unwrap();
    let e = EngineHandle::new(Project::new("keep"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("top.gbr");
    let original = source(e.read(|p| p.id));
    std::fs::write(&path, &original).unwrap();
    let before = e.session_state();
    let status = e.document_status();
    let first = e.inspect_fabrication_files(input(&e, path.clone())).unwrap();
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    assert_eq!(
        first.files[0].geometry.primitives,
        vec![FabricationPrimitive::Circle { center: Point::new(Nm(1000001), Nm(2000003)), diameter: Nm(600001) }]
    );
    let changed = original.replace("X1000001", "X1000009");
    assert_eq!(changed.len(), original.len());
    std::fs::write(&path, &changed).unwrap();
    let second = e.inspect_fabrication_files(input(&e, path.clone())).unwrap();
    assert_ne!(first.files[0].source_sha256, second.files[0].source_sha256);
    assert_ne!(first.files[0].geometry, second.files[0].geometry);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), changed);
    assert_eq!(e.session_state(), before);
    let stale = input(&e, path.clone());
    e.execute("add_part", serde_json::json!({"designator":"PEER","value":"keep","footprint":"f","x_mm":0,"y_mm":0}), Origin::agent("peer")).unwrap();
    let peer = e.session_state();
    assert!(e.inspect_fabrication_files(stale).is_err());
    assert_eq!(e.session_state(), peer);
    std::fs::write(&path, source(viamind_doc::ObjectId::new())).unwrap();
    assert!(e.inspect_fabrication_files(input(&e, path)).unwrap_err().to_string().contains("SameCoordinates"));
    assert_eq!(e.session_state(), peer);
}
#[test]
fn invalid_files_paths_and_request_limits_reject_without_partial_result() {
    let _guard = TEST.lock().unwrap();
    let e = EngineHandle::new(Project::new("keep"));
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.gbr");
    let bad = dir.path().join("bad.gbr");
    std::fs::write(&good, source(e.read(|p| p.id))).unwrap();
    std::fs::write(&bad, "G36*M02*").unwrap();
    let before = e.session_state();
    for paths in [
        vec![],
        vec![good.clone(); 7],
        vec![good.clone(), good.clone()],
        vec![good.clone(), bad],
        vec![dir.path().to_path_buf()],
        vec![std::path::PathBuf::from("x".repeat(4097))],
    ] {
        let mut request = input(&e, good.clone());
        request.paths = paths;
        assert!(e.inspect_fabrication_files(request).is_err());
        assert_eq!(e.session_state(), before);
    }
    std::fs::write(&good, vec![b' '; viamind_doc::fabrication_view::MAX_FILE_BYTES + 1]).unwrap();
    assert!(e.inspect_fabrication_files(input(&e, good)).is_err());
    assert_eq!(e.session_state(), before);
}
