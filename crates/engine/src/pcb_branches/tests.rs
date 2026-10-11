use super::*;
use serde_json::{Value, json};
use std::sync::mpsc;
use viamind_doc::{CopperLayer, Track};
use viamind_geom::{Nm, Point};

// The retained permit is process-wide, so these tests deliberately serialize.
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn saved_native_status_and_drc_journal_are_unchanged_until_merge_and_selection_stales_merge() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = engine();
    let id = e.execute("add_part", add("LIVE").input, Origin::User).unwrap().created[0];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protected.json");
    e.save_project(crate::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    let native = std::fs::read(&path).unwrap();
    let original = e.session_state();
    let status = e.document_status();
    let journal = e.auto_drc_event_state();
    let actor = BranchActor::agent(1, "native invariant");
    let b = create(&e, actor.clone());
    let a = apply(&e, &b, &actor, vec![add("PRIVATE")]).unwrap();
    diff(&e, &a.branch);
    board(&e, &a.branch, &actor);
    assert_eq!(e.session_state(), original);
    assert_eq!(e.document_status(), status);
    assert_eq!(e.auto_drc_event_state(), journal);
    assert_eq!(std::fs::read(&path).unwrap(), native);
    e.set_selection(crate::SetSelection { ids: vec![id], mode: Default::default() }, Origin::User).unwrap();
    let selected = e.session_state();
    assert!(e.merge_pcb_branch(merge(&a.branch), &actor).is_err());
    assert_eq!(e.session_state(), selected);
    // Saving the unchanged project never consumes a session cursor.
    e.discard_pcb_branch(PcbBranchRevision { branch_id: a.branch.branch_id, expected_revision: 1 }, &actor).unwrap();
    let fresh = create(&e, actor.clone());
    let fresh = apply(&e, &fresh, &actor, vec![add("PRIVATE")]).unwrap();
    e.save_project(crate::SaveProject::default()).unwrap();
    e.merge_pcb_branch(merge(&fresh.branch), &actor).unwrap();
    assert!(e.document_status().dirty);
    assert_eq!(std::fs::read(&path).unwrap(), native);
}

#[test]
fn per_step_candidate_byte_overflow_refuses_whole_private_batch_without_renewal() {
    let _lock = TEST_LOCK.lock().unwrap();
    let mut p = Project::new("x".repeat(DATA_LIMIT - 1024));
    assert!(budget::project(&p).is_ok());
    let e = EngineHandle::new(p.clone());
    let actor = BranchActor::human();
    let b = create(&e, actor.clone());
    let mut command = add("U1");
    command.input["value"] = json!("x".repeat(2048));
    assert!(apply(&e, &b, &actor, vec![command]).is_err());
    assert!(board(&e, &b, &actor).parts.is_empty());
    assert_eq!(e.list_pcb_branches()[0].revision, 0);
    assert_eq!(e.read(Clone::clone), p);
    p.name.clear();
}
fn engine() -> EngineHandle {
    EngineHandle::new(Project::new("branch contracts"))
}
fn create(e: &EngineHandle, actor: BranchActor) -> PcbBranchInfo {
    e.create_pcb_branch(
        CreatePcbBranch { expected_project: e.read(|p| p.id), expected_cursor: e.event_cursor(), label: "candidate".into() },
        actor,
        reserve_pcb_branch_work().unwrap(),
    )
    .unwrap()
}
fn call(name: &str, input: Value) -> PcbBranchCall {
    PcbBranchCall { name: name.into(), input }
}
fn add(reference: &str) -> PcbBranchCall {
    call("add_part", json!({"designator":reference,"value":"original","footprint":"authored","x_mm":1.000003,"y_mm":-2.000007}))
}
fn apply(e: &EngineHandle, b: &PcbBranchInfo, actor: &BranchActor, commands: Vec<PcbBranchCall>) -> Result<PcbBranchApplied, BranchError> {
    e.apply_pcb_branch(ApplyPcbBranch { branch_id: b.branch_id, expected_revision: b.revision, commands }, actor, reserve_pcb_branch_work().unwrap())
}
fn merge(b: &PcbBranchInfo) -> MergePcbBranch {
    MergePcbBranch { branch_id: b.branch_id, expected_revision: b.revision, expected_project: b.project_id, expected_cursor: b.source_cursor }
}
fn board(e: &EngineHandle, b: &PcbBranchInfo, actor: &BranchActor) -> Board {
    e.inspect_pcb_branch(PcbBranchId { branch_id: b.branch_id }, actor).unwrap().board
}
fn diff(e: &EngineHandle, b: &PcbBranchInfo) -> PcbBranchDiff {
    e.diff_pcb_branch(PcbBranchDiffInput { branch_id: b.branch_id, offset: 0, limit: 128 }).unwrap()
}

#[test]
fn isolated_commands_coalesce_and_merge_one_transaction_with_exact_undo_redo_ids() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = engine();
    let actor = BranchActor::agent(1, "same display");
    let before = e.session_state();
    let status = e.document_status();
    let b = create(&e, actor.clone());
    let a = apply(&e, &b, &actor, vec![add("U1"), add("U2")]).unwrap();
    let ids = a.created.clone();
    assert_eq!(ids.len(), 2);
    let a = apply(&e, &a.branch, &actor, vec![call("move_part", json!({"id":ids[0],"x_mm":-3.000001,"y_mm":4.000009}))]).unwrap();
    assert_eq!(a.branch.revision, 2);
    assert_eq!(a.branch.source_cursor, b.source_cursor);
    assert_eq!(diff(&e, &a.branch).total, 2);
    let candidate = board(&e, &a.branch, &actor);
    assert_eq!(candidate.parts[&ids[0]].position, Point::new(Nm(-3_000_001), Nm(4_000_009)));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    let committed = e.merge_pcb_branch(merge(&a.branch), &BranchActor::human()).unwrap();
    assert_eq!(committed.created, ids);
    assert_eq!(e.read(|p| p.board.clone()), candidate);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    e.with_engine(|engine| assert_eq!(engine.undo_history().len(), 1));
    assert!(e.list_pcb_branches().is_empty());
    assert!(e.merge_pcb_branch(merge(&a.branch), &actor).is_err());
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.clone()), candidate);
}

#[test]
fn ownership_atomic_errors_noops_and_exact_private_object_guards() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = engine();
    let actor = BranchActor::agent(1, "same display");
    let other = BranchActor::agent(2, "same display");
    let b = create(&e, actor.clone());
    let original = e.session_state();
    assert!(apply(&e, &b, &other, vec![add("U1")]).is_err());
    assert!(e.inspect_pcb_branch(PcbBranchId { branch_id: b.branch_id }, &other).is_err());
    assert!(e.merge_pcb_branch(merge(&b), &other).is_err());
    assert!(e.discard_pcb_branch(PcbBranchRevision { branch_id: b.branch_id, expected_revision: 0 }, &other).is_err());
    assert!(apply(&e, &b, &actor, vec![add("U1"), add("U1")]).is_err());
    assert!(apply(&e, &b, &actor, vec![call("set_board_outline", json!({}))]).is_err());
    assert!(board(&e, &b, &actor).parts.is_empty());
    assert_eq!(e.list_pcb_branches()[0].revision, 0);
    let a = apply(&e, &b, &actor, vec![add("U1")]).unwrap();
    let id = a.created[0];
    let before = board(&e, &a.branch, &actor);
    let expected = before.parts[&id].clone();
    assert!(apply(&e, &a.branch, &actor, vec![call("move_part", json!({"id":id,"x_mm":1.000003,"y_mm":-2.000007}))]).is_err());
    assert!(
        apply(
            &e,
            &a.branch,
            &actor,
            vec![call("move_part", json!({"id":id,"x_mm":5,"y_mm":6})), call("update_part", json!({"id":id,"expected":expected,"value":"stale"})),]
        )
        .is_err()
    );
    assert_eq!(board(&e, &a.branch, &actor), before);
    assert_eq!(e.list_pcb_branches()[0].revision, 1);
    assert_eq!(e.session_state(), original);
}

#[test]
fn monotonic_expiry_reads_errors_and_four_branch_capacity_do_not_renew() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = engine();
    let now = Arc::new(Mutex::new(Instant::now()));
    let clock = now.clone();
    e.pcb_branches.lock().unwrap().clock = Arc::new(move || *clock.lock().unwrap());
    let actor = BranchActor::agent(1, "lease");
    let branches: Vec<_> = (0..4).map(|_| create(&e, actor.clone())).collect();
    assert!(
        e.create_pcb_branch(
            CreatePcbBranch { expected_project: e.read(|p| p.id), expected_cursor: e.event_cursor(), label: "fifth".into() },
            actor.clone(),
            reserve_pcb_branch_work().unwrap()
        )
        .is_err()
    );
    *now.lock().unwrap() += Duration::from_secs(299);
    assert_eq!(diff(&e, &branches[0]).total, 0);
    assert!(apply(&e, &branches[0], &actor, vec![add("U1"), add("U1")]).is_err());
    let renewed = apply(&e, &branches[1], &actor, vec![add("U2")]).unwrap();
    *now.lock().unwrap() += Duration::from_secs(1);
    let remaining = e.list_pcb_branches();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].branch_id, renewed.branch.branch_id);
    assert_eq!(remaining[0].remaining_ms, 299_000);
    assert!(e.inspect_pcb_branch(PcbBranchId { branch_id: branches[0].branch_id }, &actor).is_err());
}

#[test]
fn blocked_private_worker_retains_permit_allows_live_edit_and_reopen_refuses_publish() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = engine();
    let actor = BranchActor::agent(1, "blocked");
    let b = create(&e, actor.clone());
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker_engine = e.clone();
    let worker_actor = actor.clone();
    let worker = std::thread::spawn(move || {
        worker_engine.apply_pcb_branch_with(
            ApplyPcbBranch { branch_id: b.branch_id, expected_revision: 0, commands: vec![add("U1")] },
            &worker_actor,
            reserve_pcb_branch_work().unwrap(),
            || {
                ready_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            },
        )
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(reserve_pcb_branch_work().is_err());
    assert!(e.list_pcb_branches()[0].busy);
    assert!(e.discard_pcb_branch(PcbBranchRevision { branch_id: b.branch_id, expected_revision: 0 }, &actor).is_err());
    e.execute("add_part", add("LIVE").input, Origin::User).unwrap();
    e.new_project(crate::NewProject { name: "replacement".into(), discard_dirty: true, expected_cursor: Some(e.event_cursor()) }, Origin::User)
        .unwrap();
    let after = e.session_state();
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap().is_err());
    assert_eq!(e.session_state(), after);
    assert!(!e.list_pcb_branches()[0].busy);
    assert!(e.list_pcb_branches()[0].stale);
    assert!(board(&e, &b, &actor).parts.is_empty());
    assert!(reserve_pcb_branch_work().is_ok());
}

#[test]
fn stale_live_cursor_and_wrong_merge_guards_preserve_candidate_and_peer_edits() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = engine();
    let actor = BranchActor::agent(1, "stale");
    let b = create(&e, actor.clone());
    let a = apply(&e, &b, &actor, vec![add("U1")]).unwrap();
    let mut wrong = merge(&a.branch);
    wrong.expected_cursor.sequence += 1;
    assert!(e.merge_pcb_branch(wrong, &actor).is_err());
    e.execute("add_part", add("PEER").input, Origin::User).unwrap();
    let peer = e.session_state();
    assert!(e.merge_pcb_branch(merge(&a.branch), &actor).is_err());
    assert_eq!(e.session_state(), peer);
    assert_eq!(diff(&e, &a.branch).total, 1);
    assert!(diff(&e, &a.branch).branch.stale);
    e.discard_pcb_branch(PcbBranchRevision { branch_id: b.branch_id, expected_revision: 1 }, &BranchActor::human()).unwrap();
    assert_eq!(e.session_state(), peer);
}

#[test]
fn count_before_malformed_input_and_structural_or_serialized_project_limits() {
    let _lock = TEST_LOCK.lock().unwrap();
    let oversized = json!({"commands":vec![Value::Null;65]});
    assert!(EngineHandle::validate_pcb_branch_input(&oversized).unwrap_err().message.contains("64 commands"));
    assert!(EngineHandle::validate_pcb_branch_input(&json!({"payload":"x".repeat(INPUT_LIMIT)})).is_err());
    let mut project = Project::new("rows");
    for _ in 0..=ROW_LIMIT {
        let t = Track {
            id: ObjectId::new(),
            start: Point::new(Nm(0), Nm(0)),
            end: Point::new(Nm(1), Nm(0)),
            width: Nm(1),
            layer: CopperLayer::Top,
            net: None,
        };
        project.board.tracks.insert(t.id, t);
    }
    assert!(budget::project(&project).unwrap_err().message.contains("structural rows"));
    project.board.tracks.clear();
    project.name = "x".repeat(DATA_LIMIT);
    assert!(budget::project(&project).is_err());
    let e = engine();
    let actor = BranchActor::human();
    let b = create(&e, actor.clone());
    let mut current = b;
    for batch in 0..2 {
        current = apply(&e, &current, &actor, (0..64).map(|i| add(&format!("U{}", batch * 64 + i))).collect()).unwrap().branch;
    }
    let before = board(&e, &current, &actor);
    assert!(apply(&e, &current, &actor, vec![add("U128")]).unwrap_err().message.contains("128 changed objects"));
    assert_eq!(board(&e, &current, &actor), before);
    assert_eq!(diff(&e, &current).total, 128);
    let page = e.diff_pcb_branch(PcbBranchDiffInput { branch_id: current.branch_id, offset: 0, limit: 32 }).unwrap();
    assert_eq!((page.deltas.len(), page.next_offset), (32, Some(32)));
    assert!(page.deltas.windows(2).all(|d| delta_key(&d[0]) < delta_key(&d[1])));
}
