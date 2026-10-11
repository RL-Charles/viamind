use egui::vec2;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::{ViamindApp, actions::UiAction};

#[test]
fn actual_compact_observation_scrolls_full_results_matches_shared_catalog_and_marks_peer_changes_stale() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/connectivity/one-call.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let status = engine.document_status();
    let drc = engine.auto_drc_event_state();
    let expected = UiAction::QueryUnroutedNets.run(&engine, json!({})).unwrap();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_unrouted_nets();
    let mut h = Harness::builder().with_size(vec2(640.0, 400.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h.get_by_label("Query all nets").scroll_to_me();
    h.run_steps(4);
    let rect = h.get_by_label("Query all nets").rect();
    assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= 640.0 && rect.max.y <= 400.0, "{rect:?}");
    h.get_by_label("Query all nets").click();
    h.run_steps(3);
    for _ in 0..200 {
        h.run_steps(2);
        if h.state().unrouted.report().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(serde_json::to_value(h.state().unrouted.report().unwrap()).unwrap(), expected);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert_eq!(engine.auto_drc_event_state(), drc);
    h.get_by_label("Coverage: incomplete").scroll_to_me();
    h.run_steps(5);
    assert!(h.query_by_label("Coverage: incomplete").is_some());
    let id = before.project.board.parts.keys().next().copied().unwrap();
    UiAction::SetSelection.run(&engine, json!({"ids":[id],"mode":"replace"})).unwrap();
    h.run_steps(5);
    let message = "Document or selection changed since this result; query again.";
    h.get_by_label(message).scroll_to_me();
    h.run_steps(4);
    assert!(h.query_by_label(message).is_some());
    // A human observation refresh must not inherit mutation permissions from an agent mode.
    engine.agent_policy().lock_read_only().unwrap();
    h.get_by_label("Query all nets").scroll_to_me();
    h.run_steps(4);
    assert!(!h.get_by_label("Query all nets").accesskit_node().is_disabled());
    h.get_by_label("Query all nets").click();
    h.run_steps(3);
    let cursor = engine.event_cursor();
    for _ in 0..200 {
        h.run_steps(2);
        if !h.state().unrouted.busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(h.state().unrouted.report().unwrap().source_cursor, cursor);
    assert_eq!(engine.read(Clone::clone), before.project);
    assert_eq!(engine.selection().origin, Some(Origin::User));
}
