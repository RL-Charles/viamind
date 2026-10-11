use egui::{Color32, Shape, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin, pcb_branches::*};
use viamind_ui_egui::ViamindApp;
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn proposal(e: &EngineHandle) -> PcbBranchInfo {
    let actor = BranchActor::agent(1, "branch fixture");
    let b = e
        .create_pcb_branch(
            CreatePcbBranch { expected_project: e.read(|p| p.id), expected_cursor: e.event_cursor(), label: "Authored component and copper".into() },
            actor.clone(),
            reserve_pcb_branch_work().unwrap(),
        )
        .unwrap();
    e.apply_pcb_branch(
        ApplyPcbBranch {
            branch_id: b.branch_id,
            expected_revision: 0,
            commands: vec![
                PcbBranchCall {
                    name: "add_part".into(),
                    input: json!({"designator":"U1","value":"fixture","footprint":"authored","x_mm":30,"y_mm":25}),
                },
                PcbBranchCall {
                    name: "add_track".into(),
                    input: json!({"start_x_mm":25,"start_y_mm":25,"end_x_mm":35,"end_y_mm":25,"width_mm":0.3,"layer":"top","net":"N"}),
                },
            ],
        },
        &actor,
        reserve_pcb_branch_work().unwrap(),
    )
    .unwrap()
    .branch
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    let rect = h.get_by_label(label).rect();
    assert!(rect.min.x >= 0. && rect.min.y >= 0. && rect.max.x <= 640. && rect.max.y <= 400., "{label}: {rect:?}");
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn has_color(shape: &Shape, color: Color32) -> bool {
    match shape {
        Shape::Vec(shapes) => shapes.iter().any(|s| has_color(s, color)),
        Shape::Rect(r) => r.fill == color || r.stroke.color == color,
        Shape::Circle(c) => c.fill == color || c.stroke.color == color,
        Shape::LineSegment { stroke, .. } => stroke.color == color,
        Shape::Path(p) => p.fill == color,
        _ => false,
    }
}
#[test]
fn actual_compact_review_toggle_merge_one_undo_and_no_shared_selection() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = EngineHandle::new(Project::new("desktop branch"));
    let before = e.session_state();
    let b = proposal(&e);
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_pcb_branches();
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Inspect diff");
    assert!(h.state().pcb_branches.visible());
    assert!(h.state().pcb_branches.ghost_items() > 4);
    assert!(h.output().shapes.iter().any(|s| has_color(&s.shape, Color32::from_rgba_premultiplied(30, 190, 85, 210))));
    assert_eq!(e.session_state(), before);
    click(&mut h, "Show branch diff");
    assert!(!h.state().pcb_branches.visible());
    click(&mut h, "Show branch diff");
    click(&mut h, "Merge branch");
    assert!(!h.state().pcb_branches.visible());
    assert!(h.state().pcb_branches.message().unwrap().contains("one undoable"));
    assert_eq!(e.read(|p| (p.board.parts.len(), p.board.tracks.len())), (1, 1));
    assert_eq!(e.event_cursor().sequence, b.source_cursor.sequence + 1);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.parts.len()), 1);
}
#[test]
fn actual_stale_refresh_discard_and_reopen_clear_ghost_without_mutation() {
    let _lock = TEST_LOCK.lock().unwrap();
    let e = EngineHandle::new(Project::new("desktop stale branch"));
    proposal(&e);
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_pcb_branches();
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Inspect diff");
    e.execute("add_part", json!({"designator":"PEER","value":"live","footprint":"authored","x_mm":1,"y_mm":2}), Origin::User).unwrap();
    h.run_steps(5);
    let peer = e.session_state();
    click(&mut h, "Refresh branches");
    assert!(h.get_by_label("Merge branch").accesskit_node().is_disabled());
    click(&mut h, "Merge branch");
    assert_eq!(e.session_state(), peer);
    assert_eq!(e.list_pcb_branches().len(), 1);
    click(&mut h, "Discard branch");
    assert!(e.list_pcb_branches().is_empty());
    assert_eq!(e.session_state(), peer);
    proposal(&e);
    h.run_steps(5);
    click(&mut h, "Inspect diff");
    e.new_project(
        viamind_engine::NewProject { name: "new document".into(), discard_dirty: true, expected_cursor: Some(e.event_cursor()) },
        Origin::User,
    )
    .unwrap();
    let replaced = e.session_state();
    h.run_steps(5);
    assert!(!h.state().pcb_branches.visible());
    assert_eq!(h.state().pcb_branches.ghost_items(), 0);
    assert_eq!(e.session_state(), replaced);
}
