use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_fab::{OutputJob, save_output_job};
static TEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_jobs_roundtrip_guard_admission_and_preserve_full_session() {
    let _guard = TEST.lock().await;
    let p = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(p);
    let tools = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let job_path = temp.path().join("job.json");
    let job = OutputJob::new("Agent release λ".into(), 0.050001);
    tools.call("save_output_job", json!({"path":job_path,"job":job}), Default::default()).await.unwrap();
    let loaded = tools.call("load_output_job", json!({"path":job_path}), Default::default()).await.unwrap();
    assert_eq!(loaded["job"], json!(job));
    let output = temp.path().join("release");
    let value =
        tools.call("run_output_job", json!({"job_path":job_path,"path":output,"expected_cursor":before.cursor}), Default::default()).await.unwrap();
    assert_eq!(value["exported_cursor"], json!(before.cursor));
    assert_eq!(value["file_count"], 11);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let bytes = std::fs::read(output.join("release-manifest.json")).unwrap();
    assert!(tools.call("run_output_job", json!({"job_path":job_path,"path":output}), Default::default()).await.is_err());
    assert_eq!(std::fs::read(output.join("release-manifest.json")).unwrap(), bytes);
    let a = engine.read(viamind_fab::FabricationSnapshot::capture).unwrap();
    let b = engine.read(viamind_fab::AssemblySnapshot::capture).unwrap();
    let busy = temp.path().join("busy");
    let error = tools.call("run_output_job", json!({"job_path":job_path,"path":busy}), Default::default()).await.unwrap_err();
    assert!(error.to_string().contains("busy"));
    let mut stale = before.cursor;
    stale.sequence += 1;
    let error = tools.call("run_output_job", json!({"job_path":job_path,"path":busy,"expected_cursor":stale}), Default::default()).await.unwrap_err();
    assert!(error.to_string().contains("document changed"));
    assert!(!busy.exists());
    drop((a, b));
    // A running blocking worker owns the actual job capture after its awaiter aborts.
    let captured = engine.read(|p| viamind_fab::OutputJobSnapshot::capture(p, job.clone())).unwrap();
    let held = engine.read(viamind_fab::AssemblySnapshot::capture).unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let canceled_path = temp.path().join("canceled-worker-release");
    let published = canceled_path.clone();
    let source_cursor = before.cursor;
    let task = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let result = captured.write(source_cursor, &canceled_path, None);
            finished_tx.send(result.is_ok()).unwrap();
        })
        .await
    });
    entered_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(engine.read(viamind_fab::AssemblySnapshot::capture).is_err());
    release_tx.send(()).unwrap();
    assert!(finished_rx.await.unwrap());
    assert!(published.join("release-manifest.json").is_file());
    drop(held);
    engine.execute("add_via", json!({"x_mm":1,"y_mm":1,"diameter_mm":0.6,"drill_mm":0.3}), Origin::agent("second-agent")).unwrap();
    assert!(
        tools.call("run_output_job", json!({"job_path":job_path,"path":busy,"expected_cursor":before.cursor}), Default::default()).await.is_err()
    );
    assert!(!busy.exists());
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, before.project);
    // Editing the saved descriptor is observed by the next run; no cached config is used.
    let newer = temp.path().join("newer.json");
    save_output_job(&OutputJob::new("New job".into(), 0.1), &newer, None).unwrap();
    let output = temp.path().join("newer-release");
    tools.call("run_output_job", json!({"job_path":newer,"path":output}), Default::default()).await.unwrap();
    let fab: serde_json::Value = serde_json::from_slice(&std::fs::read(output.join("fabrication/manifest.json")).unwrap()).unwrap();
    assert_eq!(fab["mask_expansion_nm"], 100000);
    assert!(tools.call("load_output_job", json!({"path":newer,"extra":true}), Default::default()).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checked_job_uses_production_drc_without_report_override_and_refuses_atomic_failures() {
    let _guard = TEST.lock().await;
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let tools = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let before = engine.session_state();
    let mut job = OutputJob::new("Checked agents".into(), 0.);
    job.profile = viamind_fab::CHECKED_PROFILE.into();
    job.checks = Some(viamind_fab::OutputJobChecks {
        copper_clearance_mm: 0.,
        edge_clearance_mm: 0.,
        minimum_via_drill_mm: 0.,
        minimum_annular_width_mm: 0.,
        hole_clearance_mm: 0.,
        hole_to_copper_clearance_mm: 0.,
        max_violations: 1000,
    });
    let job_path = temp.path().join("checked.json");
    save_output_job(&job, &job_path, None).unwrap();
    let output = temp.path().join("checked-release");
    let value =
        tools.call("run_output_job", json!({"job_path":job_path,"path":output,"expected_cursor":before.cursor}), Default::default()).await.unwrap();
    assert_eq!(value["file_count"], 12);
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(output.join("validation/drc.json")).unwrap()).unwrap();
    assert_eq!(report["exported_cursor"], json!(before.cursor));
    assert_eq!(report["report"]["violations"], json!([]));
    assert_eq!(report["report"]["coverage"]["checked_tracks"], before.project.board.tracks.len());
    assert_eq!(engine.session_state(), before);
    let failure = temp.path().join("refused");
    assert!(
        tools
            .call("run_output_job", json!({"job_path":job_path,"path":failure,"validation_report":{"pass":true}}), Default::default())
            .await
            .is_err()
    );
    assert!(!failure.exists());
    job.checks.as_mut().unwrap().edge_clearance_mm = 100.;
    let strict = temp.path().join("strict.json");
    save_output_job(&job, &strict, None).unwrap();
    let error = tools.call("run_output_job", json!({"job_path":strict,"path":failure}), Default::default()).await.unwrap_err();
    assert!(error.to_string().contains("violations"), "{error}");
    assert!(!failure.exists());
    assert!(!std::fs::read_dir(temp.path()).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".viamind-output-job-")));
    engine.execute("add_via", json!({"x_mm":1,"y_mm":1,"diameter_mm":0.6,"drill_mm":0.3}), Origin::agent("peer")).unwrap();
    let a = engine.read(viamind_fab::FabricationSnapshot::capture).unwrap();
    let b = engine.read(viamind_fab::AssemblySnapshot::capture).unwrap();
    let error = tools
        .call("run_output_job", json!({"job_path":job_path,"path":failure,"expected_cursor":before.cursor}), Default::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("document changed"));
    drop((a, b));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.session_state().project, before.project);
    assert!(!failure.exists());
}
