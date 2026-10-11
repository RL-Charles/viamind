//! Actual exact-decimal editing, compact controls and stale-session refusal.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_geom::Nm;
use viamind_ui_egui::{ViamindApp, net_rules::parse_minimum_mm};
fn setup() -> (EngineHandle, Harness<'static, ViamindApp>) {
    let e = EngineHandle::new(Project::new("Exact minima"));
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_named_net_rules();
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    (e, h)
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(4);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
#[test]
fn exact_decimal_parser_keeps_odd_nm_and_rejects_rounding_or_exponents() {
    for (text, value) in [("0", 0), ("0.000001", 1), ("0.300001", 300001), ("100.000000", 100000000)] {
        assert_eq!(parse_minimum_mm(text).unwrap(), Nm(value));
    }
    for text in ["-1", "+1", "1e-3", "NaN", ".2", "1.0000001", " 1", "100.000001", "99999999999999999999999999999999"] {
        assert!(parse_minimum_mm(text).is_err(), "{text}");
    }
}
#[test]
fn actual_compact_exact_input_atomic_apply_undo_and_cancel() {
    let (e, mut h) = setup();
    let before = e.session_state();
    click(&mut h, "Add exact net rule");
    input(&mut h, "Net 1", "USB D+");
    input(&mut h, "Clearance 1 (mm)", "0.200001");
    input(&mut h, "Track width 1 (mm)", "0.300001");
    input(&mut h, "Via drill 1 (mm)", "0.300001");
    input(&mut h, "Annular width 1 (mm)", "0.150001");
    assert_eq!(e.session_state(), before);
    for label in ["Apply named-net rules", "Reload rules", "Cancel rules"] {
        let r = h.get_by_label(label).rect();
        assert!(r.min.x >= 0. && r.min.y >= 0. && r.max.x <= 640. && r.max.y <= 400., "{label}: {r:?}");
    }
    h.state_mut().begin_route();
    h.state_mut().begin_copper();
    h.state_mut().begin_place_part();
    h.state_mut().begin_track_cleanup();
    assert!(h.state().net_rules.editing());
    assert!(!h.state().routing.previewing());
    click(&mut h, "Apply named-net rules");
    assert!(!h.state().net_rules.editing());
    let after = e.read(Clone::clone);
    let r = after.board.net_rules["USB D+"];
    assert_eq!(r.minimum_copper_clearance, Nm(200001));
    assert_eq!(r.minimum_track_width, Nm(300001));
    assert_eq!(r.minimum_via_drill, Nm(300001));
    assert_eq!(r.minimum_annular_width, Nm(150001));
    assert_eq!(viamind_doc::file::minimum_version(&after), 10);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after);
    h.state_mut().begin_named_net_rules();
    h.run_steps(4);
    input(&mut h, "Track width 1 (mm)", "0.5");
    let snapshot = e.session_state();
    click(&mut h, "Cancel rules");
    assert_eq!(e.session_state(), snapshot);
}
#[test]
fn actual_duplicate_invalid_precision_and_peer_reopen_require_reload() {
    let (e, mut h) = setup();
    click(&mut h, "Add exact net rule");
    input(&mut h, "Track width 1 (mm)", "0.0000001");
    let before = e.session_state();
    click(&mut h, "Apply named-net rules");
    assert_eq!(e.session_state(), before);
    assert!(h.state().net_rules.message().unwrap().contains("six"));
    input(&mut h, "Track width 1 (mm)", "0.3");
    click(&mut h, "Add exact net rule");
    input(&mut h, "Net 2", "N1");
    click(&mut h, "Apply named-net rules");
    assert_eq!(e.session_state(), before);
    assert!(h.state().net_rules.message().unwrap().contains("duplicate"));
    click(&mut h, "Remove rule 2");
    click(&mut h, "Add exact net rule");
    let retained = h.get_by_label("Net 2").accesskit_node().id();
    let via_field = h.get_by_label("Via drill 2 (mm)").accesskit_node().id();
    let annular_field = h.get_by_label("Annular width 2 (mm)").accesskit_node().id();
    click(&mut h, "Remove rule 1");
    assert_eq!(h.get_by_label("Net 1").accesskit_node().id(), retained, "removing another row retains the actual field identity");
    assert_eq!(h.get_by_label("Via drill 1 (mm)").accesskit_node().id(), via_field);
    assert_eq!(h.get_by_label("Annular width 1 (mm)").accesskit_node().id(), annular_field);
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"peer","footprint":"x","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    h.run_steps(3);
    let peer = e.session_state();
    click(&mut h, "Apply named-net rules");
    assert_eq!(e.session_state(), peer);
    click(&mut h, "Reload rules");
    assert!(h.query_by_label("Net 1").is_none());
    click(&mut h, "Add exact net rule");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    h.run_steps(4);
    let reopened = e.session_state();
    click(&mut h, "Apply named-net rules");
    assert_eq!(e.session_state(), reopened);
    click(&mut h, "Reload rules");
    click(&mut h, "Cancel rules");
}

#[test]
fn invalid_borrowed_policy_refuses_render_fab_and_explicit_drc_before_copy() {
    let mut project = Project::new("Capture policy");
    project.board.net_rules.insert(
        "unused".repeat(100000),
        viamind_doc::NamedNetMinima {
            minimum_copper_clearance: Nm(1),
            minimum_track_width: Nm(1),
            minimum_via_drill: viamind_geom::Nm(0),
            minimum_annular_width: viamind_geom::Nm(0),
        },
    );
    assert!(viamind_render::board_view_snapshot(&project).is_err());
    assert!(viamind_fab::FabricationSnapshot::capture(&project).is_err());
    let e = EngineHandle::new(project);
    assert!(e.query_named_net_rules().is_err());
    assert!(viamind_ui_egui::actions::UiAction::RunDrc.run(&e, serde_json::json!({})).is_err());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(e).unwrap();
    assert!(runtime.block_on(registry.call("run_drc", serde_json::json!({}), Default::default())).is_err());
}
