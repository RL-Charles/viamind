//! Actual desktop DRC rules/results, shared selection and stale snapshot presentation.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
}
#[test]
fn desktop_drc_reports_coverage_selects_issue_parts_and_marks_agent_edits_stale() {
    let engine = EngineHandle::new(Project::new("DRC UI"));
    for name in ["A", "B", "Legacy"] {
        engine.execute("add_part", json!({"designator":name,"value":"test","footprint":"explicit","x_mm":0,"y_mm":0}), Origin::User).unwrap();
        if name != "Legacy" {
            let part = engine.read(|p| p.board.part_by_designator(name).unwrap().clone());
            engine
                .execute(
                    "set_part_pads",
                    json!({"id":part.id,"expected":part,"pads":[{"number":"1","x_mm":0,"y_mm":0,"width_mm":1,"height_mm":1,"net":name}]}),
                    Origin::User,
                )
                .unwrap();
        }
    }
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    h.state_mut().begin_drc();
    h.run_steps(5);
    let before = engine.session_state();
    click(&mut h, "Run DRC");
    assert_eq!(engine.session_state(), before);
    assert!(h.query_by_label_contains("1 scoped violations").is_some());
    assert!(h.query_by_label_contains("1 excluded parts").is_some());
    assert!(h.query_by_label("Board boundary checks excluded: no explicit outline.").is_some());
    click(&mut h, "Coverage and exclusions");
    assert!(h.query_by_label_contains("legacy footprint preview").is_some());
    click(&mut h, "Select issue 1");
    assert_eq!(engine.selection().ids.len(), 2, "tree={:#?}", h.root());
    assert_eq!(engine.read(Clone::clone), before.project);
    input(&mut h, "Copper clearance (mm)", "-1");
    click(&mut h, "Run DRC");
    assert!(h.query_by_label_contains("within 0..100 mm").is_some());
    input(&mut h, "Copper clearance (mm)", "0.2");
    let id = engine.read(|p| p.board.part_by_designator("B").unwrap().id);
    engine.execute("move_part", json!({"id":id,"x_mm":10,"y_mm":0}), Origin::agent("drc-editor")).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("Results are stale: the design changed. Run DRC again.").is_some());
    click(&mut h, "Run DRC");
    assert!(h.query_by_label_contains("0 scoped violations").is_some());
    assert!(h.query_by_label("Results are stale: the design changed. Run DRC again.").is_none());
}
