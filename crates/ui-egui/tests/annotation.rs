use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;
fn engine() -> EngineHandle {
    EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json")).unwrap())
}
fn harness(engine: &EngineHandle) -> Harness<'static, ViamindApp> {
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1800.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    click(&mut h, "Annotate symbols");
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, text: &str) {
    h.get_by_label("Annotation starting number").focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label("Annotation starting number").type_text(text);
    h.run_steps(5);
}
#[test]
fn actual_preview_apply_and_undo_references_preserve_board_and_geometry() {
    let e = engine();
    let original = e.read(Clone::clone);
    let mut h = harness(&e);
    click(&mut h, "Renumber all symbols");
    input(&mut h, "10");
    let state = e.session_state();
    click(&mut h, "Prepare annotation");
    assert_eq!(e.session_state(), state);
    assert!(h.query_by_label("3 symbols, 3 proposed reference changes").is_some());
    click(&mut h, "Apply annotation");
    let project = e.read(Clone::clone);
    let symbols = &project.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().symbols;
    let refs: std::collections::BTreeSet<_> = symbols.values().map(|s| s.designator.as_str()).collect();
    assert_eq!(refs, ["C10", "C11", "U10"].into_iter().collect());
    assert_eq!(project.board, original.board);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), original);
}
#[test]
fn stale_preview_invalid_options_and_same_project_reopen_preserve_session() {
    let e = engine();
    let mut h = harness(&e);
    click(&mut h, "Renumber all symbols");
    input(&mut h, "0");
    let state = e.session_state();
    click(&mut h, "Prepare annotation");
    assert_eq!(e.session_state(), state);
    assert!(h.query_by_label("Starting number must be 1..999999999").is_some());
    input(&mut h, "5");
    click(&mut h, "Prepare annotation");
    let p = e.read(Clone::clone);
    let sheet = p.schematic.sheets.values().next().unwrap();
    e.execute(
        "edit_schematic_sheet",
        json!({"expected_project":p.id,"sheet_id":sheet.id,"expected":sheet,"name":"Agent update","edits":[]}),
        Origin::agent("other"),
    )
    .unwrap();
    h.run_steps(5);
    let state = e.session_state();
    click(&mut h, "Apply annotation");
    assert_eq!(e.session_state(), state);
    assert!(h.query_by_label_contains("prepare annotation again to reload").is_some());
    click(&mut h, "Prepare annotation");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::agent("other")).unwrap();
    h.run_steps(5);
    assert!(h.query_by_label("Apply annotation").is_none());
    assert!(h.query_by_label("Annotation starting number").is_none());
    click(&mut h, "Annotate symbols");
    assert!(h.query_by_label("Annotation starting number").is_some());
    assert!(h.query_by_label("Apply annotation").is_some());
}
