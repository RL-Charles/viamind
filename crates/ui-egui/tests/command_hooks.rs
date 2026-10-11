use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    let rect = h.get_by_label(label).rect();
    assert!(rect.min.x >= 0. && rect.min.y >= 0. && rect.max.x <= 640. && rect.max.y <= 400., "{label}: {rect:?}");
    h.get_by_label(label).click();
    h.run_steps(5);
}
#[test]
fn actual_compact_hook_controls_veto_and_recover_without_config_design_events() {
    let e = EngineHandle::new(Project::new("compact hooks"));
    let before = e.session_state();
    let status = e.document_status();
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_command_hooks();
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Enable scoped DRC veto");
    click(&mut h, "Apply hook settings");
    assert!(e.command_hook_status().config.enabled);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    for (r, clip) in h.state().command_hooks.control_rects() {
        assert!(clip.contains_rect(*r));
    }
    let input = json!({"designator":"R1","value":"","footprint":"F","x_mm":0,"y_mm":0});
    assert!(e.execute("add_part", input.clone(), Origin::User).is_err());
    h.run_steps(5);
    assert!(h.query_all_by_label("Last Pre failure: builtin_drc (details below)").next().is_some());
    assert_eq!(e.session_state(), before);
    click(&mut h, "Disable veto");
    assert!(!e.command_hook_status().config.enabled);
    e.execute("add_part", input, Origin::User).unwrap();
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
}
