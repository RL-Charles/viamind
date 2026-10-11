use egui::vec2;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{
    EngineHandle, Origin,
    agent_policy::{AgentEffect, AgentMode},
    pcb_branches::*,
};
use viamind_ui_egui::ViamindApp;
static BRANCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    let rect = h.get_by_label(label).rect();
    assert!(rect.min.x >= 0. && rect.min.y >= 0. && rect.max.x <= 640. && rect.max.y <= 400., "{label}: {rect:?}");
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn harness(app: ViamindApp) -> Harness<'static, ViamindApp> {
    Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app)
}
#[test]
fn actual_compact_controls_change_shared_policy_and_busy_or_locked_changes_refuse() {
    let engine = EngineHandle::new(Project::new("human mode controls"));
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_agent_modes();
    let mut h = harness(app);
    h.run_steps(5);
    click(&mut h, "Approval");
    assert_eq!(engine.agent_policy().status().mode, AgentMode::Approval);
    click(&mut h, "Read only");
    assert_eq!(engine.agent_policy().status().mode, AgentMode::ReadOnly);
    click(&mut h, "Live apply");
    assert_eq!(engine.agent_policy().status().mode, AgentMode::Live);
    let permit = engine.agent_policy().admit(AgentEffect::LiveWriter).unwrap().unwrap();
    click(&mut h, "Approval");
    assert_eq!(engine.agent_policy().status().mode, AgentMode::Live);
    assert!(h.query_all_by_label("1 agent operations are unfinished; retry the mode change after completion").next().is_some());
    drop(permit);
    click(&mut h, "Approval");
    assert_eq!(engine.agent_policy().status().mode, AgentMode::Approval);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    engine.agent_policy().lock_read_only().unwrap();
    h.run_steps(5);
    assert!(h.get_by_label("Live apply").accesskit_node().is_disabled());
    assert!(h.get_by_label("Approval").accesskit_node().is_disabled());
    click(&mut h, "Live apply");
    assert_eq!(engine.agent_policy().status().mode, AgentMode::ReadOnly);
    // Human edits remain available even under the permanent agent ceiling.
    engine.execute("add_part", json!({"designator":"HUMAN","value":"live","footprint":"authored","x_mm":1,"y_mm":2}), Origin::User).unwrap();
    assert_eq!(engine.event_cursor().sequence, before.cursor.sequence + 1);
}
fn proposal(engine: &EngineHandle) -> PcbBranchInfo {
    let actor = BranchActor::agent(1, "private agent");
    let branch = engine
        .create_pcb_branch(
            CreatePcbBranch { expected_project: engine.read(|p| p.id), expected_cursor: engine.event_cursor(), label: "Human review".into() },
            actor.clone(),
            reserve_pcb_branch_work().unwrap(),
        )
        .unwrap();
    engine
        .apply_pcb_branch(
            ApplyPcbBranch {
                branch_id: branch.branch_id,
                expected_revision: 0,
                commands: vec![PcbBranchCall {
                    name: "add_part".into(),
                    input: json!({"designator":format!("U{}",engine.read(|p|p.board.parts.len())+1),"value":"proposal","footprint":"authored","x_mm":3.000003,"y_mm":4.000007}),
                }],
            },
            &actor,
            reserve_pcb_branch_work().unwrap(),
        )
        .unwrap()
        .branch
}
#[test]
fn actual_approval_accept_reject_and_stale_guard_preserve_one_undo_and_peer_work() {
    let _serial = BRANCH_LOCK.lock().unwrap();
    let engine = EngineHandle::new(Project::new("Approval review"));
    engine.agent_policy().set_mode(AgentMode::Approval).unwrap();
    let before = engine.session_state();
    let branch = proposal(&engine);
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_pcb_branches();
    let mut h = harness(app);
    h.run_steps(5);
    click(&mut h, "Inspect diff");
    assert_eq!(engine.session_state(), before);
    click(&mut h, "Accept proposal");
    let accepted = engine.read(Clone::clone);
    assert_eq!(accepted.board.parts.len(), 1);
    assert_eq!(engine.event_cursor().sequence, branch.source_cursor.sequence + 1);
    engine.undo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), before.project);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(Clone::clone), accepted);
    proposal(&engine);
    h.run_steps(5);
    click(&mut h, "Inspect diff");
    let rejection_before = engine.session_state();
    click(&mut h, "Reject proposal");
    assert_eq!(engine.session_state(), rejection_before);
    assert!(engine.list_pcb_branches().is_empty());
    proposal(&engine);
    h.run_steps(5);
    click(&mut h, "Inspect diff");
    engine.execute("add_part", json!({"designator":"PEER","value":"preserve","footprint":"authored","x_mm":7,"y_mm":8}), Origin::User).unwrap();
    let peer = engine.session_state();
    h.run_steps(5);
    click(&mut h, "Refresh branches");
    assert!(h.get_by_label("Accept proposal").accesskit_node().is_disabled());
    click(&mut h, "Accept proposal");
    assert_eq!(engine.session_state(), peer);
    assert_eq!(engine.list_pcb_branches().len(), 1);
    click(&mut h, "Reject proposal");
    assert_eq!(engine.session_state(), peer);
}
