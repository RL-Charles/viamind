use serde_json::json;
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{CallContext, engine_registry};

#[tokio::test]
async fn exports_shared_snapshot_and_rejects_stale_guards_and_bad_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().join("project.json");
    file::save(&Project::new("exports"), &native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let registry = engine_registry(engine.clone()).unwrap();
    let baseline = engine.session_state().cursor;
    engine
        .execute(
            "add_part",
            json!({"designator":"R1","value":"10k","footprint":"0603","x_mm":1.234567,"y_mm":-2,"rotation_deg":45,"side":"bottom"}),
            Origin::agent("placing-agent"),
        )
        .unwrap();
    let output = directory.path().join("parts.csv");
    assert!(
        registry.call("export_assembly_csv", json!({"kind":"bom","path":output,"expected_cursor":baseline}), CallContext::default()).await.is_err()
    );
    assert!(!output.exists());
    let before = engine.session_state();
    let status = engine.document_status();
    let result = registry
        .call("export_assembly_csv", json!({"kind":"placement","path":output,"expected_cursor":before.cursor}), CallContext::default())
        .await
        .unwrap();
    assert_eq!(result["exported_cursor"], json!(before.cursor));
    assert_eq!(result["component_count"], 1);
    assert!(std::fs::read_to_string(&output).unwrap().contains("1.234567,-2.000000,45.000,Bottom"));
    assert!(registry.call("export_assembly_csv", json!({"kind":"bom","path":output}), CallContext::default()).await.is_err());
    registry.call("export_assembly_csv", json!({"kind":"bom","path":output,"overwrite":true}), CallContext::default()).await.unwrap();
    let native_bytes = std::fs::read(&native).unwrap();
    assert!(registry.call("export_assembly_csv", json!({"kind":"bom","path":native,"overwrite":true}), CallContext::default()).await.is_err());
    assert_eq!(std::fs::read(native).unwrap(), native_bytes);
    for input in [
        json!({"kind":"bogus","path":output}),
        json!({"kind":"bom","path":""}),
        json!({"kind":"bom","path":output,"unexpected":1}),
        json!({"kind":"bom"}),
        json!({"kind":"bom","path":output,"overwrite":"yes"}),
    ] {
        assert!(registry.call("export_assembly_csv", input, CallContext::default()).await.is_err());
    }
    assert_eq!(engine.session_state(), before, "exports add no events/history/selection edits");
    assert_eq!(engine.document_status(), status, "exports do not save the native design");
}
