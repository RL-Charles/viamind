use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_fab::AssemblyCsv;

#[tokio::test]
async fn calibrated_shared_setter_two_clients_full_guards_and_publication_are_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    let initial = EngineHandle::new(viamind_doc::Project::new("Agents"));
    let id = initial
        .execute(
            "add_part",
            json!({"designator":"U1","value":"µ, demo","footprint":"asymmetric","x_mm":10,"y_mm":20,"rotation_deg":90,"side":"bottom"}),
            Origin::User,
        )
        .unwrap()
        .created[0];
    initial.read(|p| viamind_doc::file::save(p, &native)).unwrap();
    let native_bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let a = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let b = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let original = engine.read(|p| p.board.parts[&id].clone());
    let original_cursor = engine.event_cursor();
    let metadata = json!({"mounting":"through_hole","placement":{"centroid_local":{"x":2_000_000,"y":1_000_000},"zero_axis_mdeg":37_125}});
    a.call("set_part_assembly", json!({"id":id,"assembly":metadata,"expected":original}), Default::default()).await.unwrap();
    let state = engine.session_state();
    let observer = a.clone().read_only();
    assert!(observer.get("set_part_assembly").is_none());
    assert!(observer.get("export_assembly_csv").is_none());
    let observed = observer.call("query_parts", json!({}), Default::default()).await.unwrap();
    assert_eq!(observed["parts"][0]["assembly"], metadata);
    let forbidden = temp.path().join("observer.csv");
    assert!(
        observer
            .call("export_assembly_csv", json!({"kind":"assembly_placement","path":forbidden,"overwrite":true}), Default::default())
            .await
            .is_err()
    );
    assert!(observer.call("set_part_assembly", json!({"id":id,"assembly":null,"expected":original}), Default::default()).await.is_err());
    assert!(!forbidden.exists());
    assert_eq!(engine.session_state(), state);
    assert!(b.call("set_part_assembly", json!({"id":id,"assembly":null,"expected":original}), Default::default()).await.is_err());
    let stale = temp.path().join("stale.csv");
    assert!(
        b.call("export_assembly_csv", json!({"kind":"assembly_placement","path":stale,"expected_cursor":original_cursor}), Default::default())
            .await
            .is_err()
    );
    assert!(!stale.exists());
    assert_eq!(engine.session_state(), state);
    let path = temp.path().join("calibrated.csv");
    let request = json!({"kind":"assembly_placement","path":path,"expected_cursor":state.cursor});
    let result = a.call("export_assembly_csv", request.clone(), Default::default()).await.unwrap();
    assert_eq!(result["component_count"], 1);
    assert_eq!(result["exported_cursor"], json!(state.cursor));
    let expected = engine.read(|p| viamind_fab::csv(p, AssemblyCsv::AssemblyPlacement)).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    assert!(b.call("export_assembly_csv", request, Default::default()).await.is_err());
    b.call("export_assembly_csv", json!({"kind":"assembly_placement","path":path,"overwrite":true}), Default::default()).await.unwrap();
    assert!(b.call("export_assembly_csv", json!({"kind":"assembly_placement","path":native,"overwrite":true}), Default::default()).await.is_err());
    assert_eq!(std::fs::read(&native).unwrap(), native_bytes);
    assert_eq!(engine.session_state(), state);
    engine.undo(Origin::User).unwrap();
    assert!(engine.read(|p| p.board.parts[&id].assembly.is_none()));
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.parts[&id].assembly.clone()), state.project.board.parts[&id].assembly);
}
