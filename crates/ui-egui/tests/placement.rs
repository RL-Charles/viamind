//! Actual desktop pointer, keyboard and menu inputs drive one shared guarded placement command.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_doc::{ObjectId, Project, Side};
use viamind_engine::{EngineHandle, Origin, SelectionMode, SetSelection};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
fn setup() -> (EngineHandle, Vec<ObjectId>, Harness<'static, ViamindApp>) {
    let e = EngineHandle::new(Project::new("Drag"));
    let ids = [(20, 30), (30, 35), (45, 40)]
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| {
            e.execute("add_part", json!({"designator":format!("R{i}"),"value":"1k","footprint":"R_0603","x_mm":x,"y_mm":y}), Origin::User)
                .unwrap()
                .created[0]
        })
        .collect::<Vec<_>>();
    e.set_selection(SetSelection { ids: ids[..2].to_vec(), mode: SelectionMode::Replace }, Origin::User).unwrap();
    let mut h = Harness::builder()
        .with_size(vec2(1400.0, 1000.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(e.clone(), None));
    h.run_steps(5);
    (e, ids, h)
}
fn screen(h: &Harness<'_, ViamindApp>, x: f64, y: f64) -> Pos2 {
    h.state().canvas.camera.world_to_screen(WorldPos::new(x, y), h.state().canvas.viewport)
}
fn button(h: &mut Harness<'_, ViamindApp>, pos: Pos2, pressed: bool) {
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.run_steps(2);
}
fn key(h: &mut Harness<'_, ViamindApp>, key: Key) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}
fn begin_drag(h: &mut Harness<'_, ViamindApp>) -> Pos2 {
    let start = screen(h, 20.0, 30.0);
    button(h, start, true);
    let end = screen(h, 25.0, 37.0);
    h.event(Event::PointerMoved(end));
    h.run_steps(2);
    end
}
fn menu(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
#[test]
fn pointer_move_previews_without_mutation_and_release_commits_one_group() {
    let (e, ids, mut h) = setup();
    let before = e.session_state();
    let end = begin_drag(&mut h);
    assert!(h.state().selection.placement.moving());
    assert_eq!(e.session_state(), before, "local ghost cannot change shared document/events/history");
    button(&mut h, end, false);
    assert!(!h.state().selection.placement.moving());
    let after = e.read(Clone::clone);
    for id in &ids[..2] {
        let a = after.board.part(id).unwrap();
        let b = before.project.board.part(id).unwrap();
        assert!((a.position.x.0 - b.position.x.0 - 5000000).abs() < 1000);
        assert!((a.position.y.0 - b.position.y.0 - 7000000).abs() < 1000);
    }
    assert_eq!(after.board.part(&ids[2]), before.project.board.part(&ids[2]));
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(Clone::clone), before.project);
}
#[test]
fn escape_space_selection_change_and_concurrent_agent_edit_cancel_or_reject_whole_drag() {
    let (e, ids, mut h) = setup();
    let before = e.session_state();
    let end = begin_drag(&mut h);
    key(&mut h, Key::Escape);
    button(&mut h, end, false);
    assert_eq!(e.session_state(), before);
    let end = begin_drag(&mut h);
    key(&mut h, Key::Space);
    button(&mut h, end, false);
    assert_eq!(e.session_state(), before);
    let end = begin_drag(&mut h);
    h.event(Event::WindowFocused(false));
    h.run_steps(2);
    button(&mut h, end, false);
    assert_eq!(e.session_state(), before);
    h.event(Event::WindowFocused(true));
    h.run_steps(2);
    // Reset the camera after the Space pan, then start another group drag.
    h.state_mut().canvas.fit_pending = true;
    h.run_steps(3);
    let end = begin_drag(&mut h);
    e.execute("move_part", json!({"id":ids[1],"x_mm":31,"y_mm":35}), Origin::agent("concurrent")).unwrap();
    let latest = e.read(Clone::clone);
    button(&mut h, end, false);
    assert_eq!(e.read(Clone::clone), latest);
    assert!(h.state().selection.placement.error.as_ref().unwrap().contains("changed"));
    let end = begin_drag(&mut h);
    e.set_selection(SetSelection { ids: vec![ids[2]], mode: SelectionMode::Replace }, Origin::agent("selector")).unwrap();
    h.run_steps(2);
    button(&mut h, end, false);
    assert_eq!(e.read(Clone::clone), latest);
    assert_eq!(e.selection().ids, [ids[2]]);
}
#[test]
fn canvas_keys_rotate_flip_and_property_text_or_modal_focus_suspends_shortcuts() {
    let (e, ids, mut h) = setup();
    let pointer = screen(&h, 20.0, 30.0);
    h.event(Event::PointerMoved(pointer));
    h.run_steps(2);
    key(&mut h, Key::R);
    key(&mut h, Key::F);
    e.read(|p| {
        for id in &ids[..2] {
            let part = p.board.part(id).unwrap();
            assert_eq!(part.rotation_mdeg, 90000);
            assert_eq!(part.side, Side::Bottom);
        }
    });
    e.set_selection(SetSelection { ids: vec![ids[0]], mode: SelectionMode::Replace }, Origin::User).unwrap();
    h.run_steps(4);
    h.get_by_label("Designator").focus();
    h.run_steps(2);
    key(&mut h, Key::R);
    assert_eq!(e.read(|p| p.board.part(&ids[0]).unwrap().rotation_mdeg), 90000);
    h.state_mut().begin_place_part();
    h.run_steps(5);
    key(&mut h, Key::F);
    assert_eq!(e.read(|p| p.board.part(&ids[0]).unwrap().side), Side::Bottom);
}
#[test]
fn actual_arrange_menu_aligns_and_distributes_selected_origins() {
    let (e, ids, mut h) = setup();
    e.set_selection(SetSelection { ids: ids.clone(), mode: SelectionMode::Replace }, Origin::User).unwrap();
    h.run_steps(3);
    menu(&mut h, "Edit");
    menu(&mut h, "Arrange part origins ⏵");
    menu(&mut h, "Align origins Y minimum");
    assert!(e.read(|p| p.board.parts.values().all(|p| p.position.y.0 == 30000000)));
    menu(&mut h, "Edit");
    menu(&mut h, "Arrange part origins ⏵");
    menu(&mut h, "Distribute origins X");
    assert_eq!(e.read(|p| p.board.part(&ids[1]).unwrap().position.x.0), 32500000);
    h.state_mut().undo_design();
    h.run_steps(3);
    assert_eq!(e.read(|p| p.board.part(&ids[1]).unwrap().position.x.0), 30000000);
}

#[test]
fn copper_modal_cancels_active_preview_and_blocks_canvas_and_menu_transforms() {
    let (e, _, mut h) = setup();
    let before = e.session_state();
    let end = begin_drag(&mut h);
    h.state_mut().begin_copper();
    h.run_steps(3);
    assert!(!h.state().selection.placement.moving());
    button(&mut h, end, false);
    key(&mut h, Key::R);
    key(&mut h, Key::F);
    assert_eq!(e.session_state(), before);
    menu(&mut h, "Edit");
    assert!(h.query_by_label("Rotate selected 90° (R)").is_none(), "copper modal blocks placement menu access");
    assert_eq!(e.session_state(), before);
}
