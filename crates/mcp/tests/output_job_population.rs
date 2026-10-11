use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_fab::{AssemblyCsv, OutputJob, OutputJobChecks, OutputJobSnapshot};
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn matched_population_shared_job_keeps_exact_capture_rules_publication_and_read_only_contracts() {
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    for part in project.board.parts.values_mut() {
        part.assembly = Some(Box::new(
            serde_json::from_value(
                json!({"manufacturer":"Capture-before λ","mounting":"surface_mount","placement":{"centroid_local":{"x":2000000,"y":1000000},"zero_axis_mdeg":37125}}),
            )
            .unwrap(),
        ));
    }
    project.board.net_rules.insert(
        "GND".into(),
        serde_json::from_value(
            json!({"minimum_copper_clearance":0,"minimum_track_width":250000,"minimum_via_drill":300000,"minimum_annular_width":250000}),
        )
        .unwrap(),
    );
    let temp = tempfile::tempdir().unwrap();
    let native = temp.path().join("native.json");
    viamind_doc::file::save(&project, &native).unwrap();
    let native_bytes = std::fs::read(&native).unwrap();
    let engine = EngineHandle::load(&native).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let a = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let b = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let mut job = OutputJob::new("Calibrated shared checked job".into(), 0.);
    job.version = 3;
    job.include_populated_bom = Some(true);
    job.include_calibrated_placement = Some(true);
    job.profile = viamind_fab::CHECKED_PROFILE.into();
    job.checks = Some(OutputJobChecks {
        copper_clearance_mm: 0.,
        edge_clearance_mm: 0.,
        minimum_via_drill_mm: 0.,
        minimum_annular_width_mm: 0.,
        hole_clearance_mm: 0.,
        hole_to_copper_clearance_mm: 0.,
        max_violations: 1000,
    });
    let job_path = temp.path().join("job.json");
    a.call("save_output_job", json!({"path":job_path,"job":job}), Default::default()).await.unwrap();
    assert_eq!(b.call("load_output_job", json!({"path":job_path}), Default::default()).await.unwrap()["job"], json!(job));
    for (name, tools) in [("a", &a), ("b", &b)] {
        let path = temp.path().join(name);
        let result =
            tools.call("run_output_job", json!({"job_path":job_path,"path":path,"expected_cursor":before.cursor}), Default::default()).await.unwrap();
        assert_eq!(result["file_count"], 14);
        assert_eq!(result["manifest"]["populated_assembly"]["component_count"], 2);
        assert_eq!(result["manifest"]["populated_assembly"]["component_ids_sha256"].as_str().unwrap().len(), 64);
        assert!(std::fs::read_to_string(path.join("assembly/populated-bom.csv")).unwrap().contains("Capture-before λ"));
        assert_eq!(result["manifest"]["calibrated_placement"]["component_count"], 2);
        assert_eq!(result["exported_cursor"], json!(before.cursor));
        assert_eq!(
            std::fs::read_to_string(path.join("assembly/calibrated-placement.csv")).unwrap(),
            viamind_fab::csv(&before.project, AssemblyCsv::AssemblyPlacement).unwrap()
        );
        let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join("validation/drc.json")).unwrap()).unwrap();
        assert_eq!(report["report"]["document_net_rules"], json!(before.project.board.net_rules));
        assert_eq!(report["report"]["violations"], json!([]));
    }
    for dir in ["", "assembly", "fabrication", "validation"] {
        for entry in std::fs::read_dir(temp.path().join("a").join(dir)).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() {
                assert_eq!(std::fs::read(entry.path()).unwrap(), std::fs::read(temp.path().join("b").join(dir).join(entry.file_name())).unwrap());
            }
        }
    }
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let observer = a.clone().read_only();
    assert!(observer.get("run_output_job").is_none() && observer.get("save_output_job").is_none());
    let forbidden = temp.path().join("observer");
    assert!(observer.call("run_output_job", json!({"job_path":job_path,"path":forbidden}), Default::default()).await.is_err());
    assert!(!forbidden.exists());
    let collision = std::fs::read(temp.path().join("a/release-manifest.json")).unwrap();
    assert!(a.call("run_output_job", json!({"job_path":job_path,"path":temp.path().join("a")}), Default::default()).await.is_err());
    assert_eq!(std::fs::read(temp.path().join("a/release-manifest.json")).unwrap(), collision);
    assert!(a.call("run_output_job", json!({"job_path":job_path,"path":native}), Default::default()).await.is_err());
    let snapshot = engine.read(|p| OutputJobSnapshot::capture(p, job.clone())).unwrap();
    engine.execute("set_named_net_rules",json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"rules":[{"net":"GND","minimum_copper_clearance":0,"minimum_track_width":250001}]}),Origin::agent("peer")).unwrap();
    let second = engine.read(viamind_fab::FabricationSnapshot::capture).unwrap();
    let stale = temp.path().join("stale");
    let error =
        a.call("run_output_job", json!({"job_path":job_path,"path":stale,"expected_cursor":before.cursor}), Default::default()).await.unwrap_err();
    assert!(error.to_string().contains("document changed"), "{error}");
    assert!(!stale.exists());
    drop(second);
    let current = engine.session_state();
    let refused = temp.path().join("policy-refused");
    assert!(
        a.call("run_output_job", json!({"job_path":job_path,"path":refused,"expected_cursor":current.cursor}), Default::default())
            .await
            .unwrap_err()
            .to_string()
            .contains("track width")
    );
    assert!(!refused.exists());
    assert_eq!(engine.session_state(), current);
    let captured = temp.path().join("captured");
    snapshot.write_with_validation(before.cursor, &captured, Some(&native), viamind_engine::checked_output_job_report).unwrap();
    assert_eq!(
        std::fs::read_to_string(captured.join("assembly/calibrated-placement.csv")).unwrap(),
        viamind_fab::csv(&before.project, AssemblyCsv::AssemblyPlacement).unwrap()
    );
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(captured.join("validation/drc.json")).unwrap()).unwrap();
    assert_eq!(report["report"]["document_net_rules"], json!(before.project.board.net_rules));
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before.project);
    assert_eq!(std::fs::read(&native).unwrap(), native_bytes);
    // The calibrated profile shares the newly merged document via policies too.
    for (name, drill, annulus) in [("drill-floor", 300001, 250000), ("annular-floor", 300000, 250001)] {
        let cursor = engine.event_cursor();
        engine.execute("set_named_net_rules", json!({"expected_project":before.project.id,"expected_cursor":cursor,"rules":[{"net":"GND","minimum_copper_clearance":0,"minimum_track_width":250000,"minimum_via_drill":drill,"minimum_annular_width":annulus}]}), Origin::agent("peer")).unwrap();
        let current = engine.session_state();
        let refused = temp.path().join(name);
        let error = a
            .call("run_output_job", json!({"job_path":job_path,"path":refused,"expected_cursor":current.cursor}), Default::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("violations"), "{error}");
        assert!(!refused.exists());
        assert_eq!(engine.session_state(), current);
        assert!(!std::fs::read_dir(temp.path()).unwrap().any(|p| p.unwrap().file_name().to_string_lossy().starts_with(".viamind-output-job-")));
        engine.undo(Origin::User).unwrap();
        assert_eq!(engine.read(Clone::clone), before.project);
    }
    // A canceled async awaiter cannot release the slot or change a running job's owned capture.
    let retained = engine.read(|p| OutputJobSnapshot::capture(p, job)).unwrap();
    let current = engine.session_state();
    let cursor = current.cursor;
    let published = temp.path().join("canceled-awaiter");
    let output = published.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let result = retained.write_with_validation(cursor, &output, None, viamind_engine::checked_output_job_report);
            done_tx.send(result.is_ok()).unwrap();
        })
        .await
    });
    ready_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let part = current.project.board.parts.values().next().unwrap();
    let mut metadata = part.assembly.as_deref().unwrap().clone();
    metadata.manufacturer = Some("Edited-after-capture".into());
    engine
        .execute(
            "set_part_assembly",
            json!({"id":part.id,"assembly":metadata,"expected":part,"expected_project":current.project.id}),
            Origin::agent("sourcing-peer"),
        )
        .unwrap();
    let edited = engine.session_state();
    let other = engine.read(viamind_fab::FabricationSnapshot::capture).unwrap();
    assert_eq!(engine.read(viamind_fab::AssemblySnapshot::capture).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    drop(other);
    release_tx.send(()).unwrap();
    assert!(done_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap());
    assert!(published.join("assembly/calibrated-placement.csv").exists());
    let bom = std::fs::read_to_string(published.join("assembly/populated-bom.csv")).unwrap();
    assert!(bom.contains("Capture-before λ") && !bom.contains("Edited-after-capture"));
    assert_eq!(engine.session_state(), edited);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), current.project);
    assert_eq!(std::fs::read(native).unwrap(), native_bytes);
}
