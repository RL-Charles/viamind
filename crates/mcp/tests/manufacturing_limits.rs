use serde_json::json;
use viamind_doc::{ObjectId, Project, Sheet, file};
use viamind_engine::{EngineHandle, EventCursor};
use viamind_fab::{AssemblySnapshot, FabricationSnapshot};
use viamind_mcp::{CallContext, engine_registry};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_capture_busy_stale_and_canceled_worker_ownership_preserve_engine() {
    let mut project = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let id = project.board.parts.values().next().unwrap().pads.as_ref().unwrap().0[0].id;
    let mut sheet = Sheet::new("unrelated".repeat(500_000));
    sheet.id = ObjectId::new();
    project.schematic.sheets.insert(id, sheet);
    let engine = EngineHandle::new(project);
    let registry = engine_registry(engine.clone()).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    let a = engine.read(AssemblySnapshot::capture).unwrap();
    let b = engine.read(FabricationSnapshot::capture).unwrap();
    for (tool, input) in [
        ("export_assembly_csv", json!({"kind":"bom","path":dir.path().join("busy.csv")})),
        ("export_fabrication", json!({"path":dir.path().join("busy-fab")})),
    ] {
        let error = registry.call(tool, input, CallContext::default()).await.unwrap_err();
        assert!(error.to_string().contains("busy"), "{error}");
    }
    let stale = EventCursor { stream_id: before.cursor.stream_id, sequence: before.cursor.sequence + 1 };
    let error = registry
        .call("export_assembly_csv", json!({"kind":"bom","path":dir.path().join("stale.csv"),"expected_cursor":stale}), CallContext::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("document changed"), "guard precedes capture/admission: {error}");
    // Same production ownership pattern: the captured snapshot lives inside spawn_blocking.
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            let snapshot = a;
            let _ = entered_tx.send(());
            release_rx.recv().unwrap();
            drop(snapshot);
            let _ = finished_tx.send(());
        })
        .await
    });
    entered_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(AssemblySnapshot::capture(&Project::new("busy")).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    release_tx.send(()).unwrap();
    finished_rx.await.unwrap();
    drop(b);
    let csv = dir.path().join("parts.csv");
    let fab = dir.path().join("fab");
    let result =
        registry.call("export_assembly_csv", json!({"kind":"bom","path":csv,"expected_cursor":before.cursor}), CallContext::default()).await.unwrap();
    assert_eq!(result["exported_cursor"], json!(before.cursor));
    let result = registry.call("export_fabrication", json!({"path":fab,"expected_cursor":before.cursor}), CallContext::default()).await.unwrap();
    assert_eq!(result["exported_cursor"], json!(before.cursor));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
