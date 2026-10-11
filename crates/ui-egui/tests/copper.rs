//! Drive the accessible coordinate form, shared state and rendered dimensions.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::Project;
use viamind_engine::{EngineHandle, NewProject, Origin};
use viamind_ui_egui::{ViamindApp, layers::LayerId, scene::Prim};
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(5);
}
#[test]
fn coordinate_form_adds_tracks_and_vias_deletes_and_rejects_stale_document() {
    let engine = EngineHandle::new(Project::new("Copper UI"));
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_copper();
    let mut h = Harness::builder().with_size(vec2(1500.0, 1100.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    input(&mut h, "Track width (mm)", "-1");
    click(&mut h, "Add copper");
    assert!(engine.read(|p| p.board.tracks.is_empty()));
    input(&mut h, "Track width (mm)", "0.250001");
    input(&mut h, "Copper net (optional)", "GND");
    click(&mut h, "Bottom copper");
    click(&mut h, "Add copper");
    let track = engine.read(|p| p.board.tracks.values().next().unwrap().clone());
    assert_eq!(track.width.0, 250001);
    assert_eq!(track.net.as_deref(), Some("GND"));
    assert!(
        h.state()
            .scene
            .items
            .iter()
            .any(|item| item.layer == LayerId::B_CU && matches!(item.prim,Prim::Segment{width,..} if (width-0.250001).abs()<1e-12))
    );
    click(&mut h, "Through via");
    input(&mut h, "Via drill (mm)", "0.6");
    click(&mut h, "Add copper");
    assert!(engine.read(|p| p.board.vias.is_empty()));
    input(&mut h, "Via drill (mm)", "0.3");
    click(&mut h, "Add copper");
    let via = engine.read(|p| p.board.vias.values().next().unwrap().clone());
    assert!(h.state().scene.items.iter().any(|item| matches!(item.prim,Prim::Via{radius,drill_radius,..} if radius==0.3 && drill_radius==0.15)));
    click(&mut h, &format!("Delete track {}", track.id));
    assert!(engine.read(|p| p.board.tracks.is_empty()));
    click(&mut h, &format!("Delete via {}", via.id));
    assert!(engine.read(|p| p.board.vias.is_empty()));
    click(&mut h, "Close copper");
    h.state_mut().undo_design();
    h.state_mut().undo_design();
    h.run_steps(5);
    assert!(engine.read(|p| p.board.vias.contains_key(&via.id) && p.board.tracks.contains_key(&track.id)));
    h.state_mut().begin_copper();
    h.run_steps(5);
    let before = engine.session_state();
    engine
        .new_project(NewProject { name: "Replacement".into(), discard_dirty: true, expected_cursor: Some(before.cursor) }, Origin::agent("opener"))
        .unwrap();
    click(&mut h, "Add copper");
    assert!(engine.read(|p| p.board.tracks.is_empty()));
    click(&mut h, "Close copper");
}
