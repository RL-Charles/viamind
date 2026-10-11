use serde_json::json;
use viamind_engine::EngineHandle;
use viamind_mcp::{agent::CallContext, engine_registry};
#[tokio::test]
async fn snapshot_export_is_shared_read_only_and_rejects_stale_or_collision() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let tools = engine_registry(engine.clone()).unwrap();
    let agent = CallContext::new("fabrication-agent");
    let before = engine.session_state();
    let status = engine.document_status();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fab");
    let result =
        tools.call("export_fabrication", json!({"path":path,"expected_cursor":before.cursor,"mask_expansion_mm":0.1}), agent.clone()).await.unwrap();
    assert_eq!(result["manifest"]["through_via_count"], 2);
    assert_eq!(result["exported_cursor"], json!(before.cursor));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert!(tools.call("export_fabrication", json!({"path":path}), agent.clone()).await.is_err());
    engine.execute("add_via", json!({"x_mm":1,"y_mm":2,"diameter_mm":0.6,"drill_mm":0.3}), viamind_engine::Origin::agent("other")).unwrap();
    let stale = directory.path().join("stale");
    assert!(tools.call("export_fabrication", json!({"path":stale,"expected_cursor":before.cursor}), agent.clone()).await.is_err());
    assert!(!stale.exists());
    assert!(tools.call("export_fabrication", json!({"path":directory.path().join("bad"),"overwrite":true}), agent).await.is_err());
}
