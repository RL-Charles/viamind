use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_fab::{AssemblyCsv, AssemblySnapshot};
#[tokio::test]
async fn two_agents_template_counts_guards_and_publication_match_shared_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    let source = include_str!("../../../fixtures/assembly/bom-templates.viamind.json");
    std::fs::write(&native, source).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let a = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let b = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let baseline = engine.session_state();
    let status = engine.document_status();
    for kind in [AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom] {
        let path = temp.path().join(format!("{}.csv", kind.name()));
        let request = json!({"kind":kind,"path":path,"expected_cursor":baseline.cursor});
        let result = a.call("export_assembly_csv", request.clone(), Default::default()).await.unwrap();
        assert_eq!(result["kind"], json!(kind));
        assert_eq!(result["component_count"], 1);
        assert_eq!(result["exported_cursor"], json!(baseline.cursor));
        let expected = engine.read(|p| viamind_fab::csv(p, kind)).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        assert!(b.call("export_assembly_csv", request, Default::default()).await.is_err());
        b.call("export_assembly_csv", json!({"kind":kind,"path":path,"overwrite":true}), Default::default()).await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        assert!(a.call("export_assembly_csv", json!({"kind":kind,"path":native,"overwrite":true}), Default::default()).await.is_err());
    }
    assert_eq!(engine.session_state(), baseline);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read_to_string(native).unwrap(), source);
    let id = engine.read(|p| p.board.parts.values().find(|p| p.assembly.as_ref().unwrap().populate).unwrap().id);
    engine.execute("update_part", json!({"id":id,"value":"peer"}), Origin::agent("b")).unwrap();
    let first = engine.read(AssemblySnapshot::capture).unwrap();
    let second = engine.read(AssemblySnapshot::capture).unwrap();
    let path = temp.path().join("stale.csv");
    let error = a
        .call("export_assembly_csv", json!({"kind":"catalog_bom","path":path,"expected_cursor":baseline.cursor}), Default::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("document changed"));
    assert!(!path.exists());
    drop((first, second));
    engine.execute("set_part_assembly", json!({"id":id,"assembly":{"mpn":"Explicit only"}}), Origin::agent("b")).unwrap();
    let before = engine.session_state();
    assert!(
        a.call("export_assembly_csv", json!({"kind":"catalog_bom","path":path}), Default::default())
            .await
            .unwrap_err()
            .to_string()
            .contains("supplier")
    );
    assert_eq!(engine.session_state(), before);
    assert!(!path.exists());
}
