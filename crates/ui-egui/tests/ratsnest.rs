//! The actual desktop scene follows shared connectivity edits and undo.
use egui::vec2;
use egui_kittest::Harness;
use serde_json::json;
use viamind_doc::Project;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::{ViamindApp, scene::Prim};
#[test]
fn native_scene_guides_follow_pad_geometry_net_assignment_and_history() {
    let engine = EngineHandle::new(Project::new("Ratsnest"));
    let mut ids = Vec::new();
    for (name, x) in [("R1", 10), ("R2", 20)] {
        let id = engine
            .execute("add_part", json!({"designator":name,"value":"1k","footprint":"R_0603","x_mm":x,"y_mm":20}), Origin::User)
            .unwrap()
            .created[0];
        let expected = engine.read(|p| p.board.part(&id).unwrap().clone());
        engine
            .execute(
                "set_part_pads",
                json!({"id":id,"expected":expected,"pads":[{"number":"1","x_mm":2,"y_mm":1,"width_mm":2,"height_mm":1,"net":"GND"}]}),
                Origin::User,
            )
            .unwrap();
        ids.push(id);
    }
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1200.0, 900.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    assert_eq!(h.state().scene.ratsnest.len(), 1);
    let initial = h.state().scene.ratsnest.clone();
    engine.execute("update_part", json!({"id":ids[0],"x_mm":30,"y_mm":40,"rotation_deg":45,"side":"bottom"}), Origin::agent("geometry")).unwrap();
    h.run_steps(5);
    assert_ne!(h.state().scene.ratsnest, initial);
    let expected = engine.read(|p| p.board.part(&ids[0]).unwrap().clone());
    let centre = expected.local_to_world_mm(2.0, 1.0);
    let edge = h.state().scene.ratsnest[0];
    assert!(edge.iter().any(|p| (p.x, p.y) == centre));
    // Copper corners and the query-derived guide agree at an arbitrary mirrored rotation.
    assert!(h.state().scene.items.iter().any(|item| match item.prim {
        Prim::Quad(points) =>
            ((points.iter().map(|p| p.x).sum::<f64>() / 4.0) - centre.0).abs() < 1e-9
                && ((points.iter().map(|p| p.y).sum::<f64>() / 4.0) - centre.1).abs() < 1e-9,
        _ => false,
    }));
    engine.execute("set_part_pads", json!({"id":ids[0],"expected":expected,"pads":[]}), Origin::agent("electrical")).unwrap();
    h.run_steps(5);
    assert!(h.state().scene.ratsnest.is_empty());
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(h.state().scene.ratsnest.len(), 1);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(h.state().scene.ratsnest, initial);
    engine.redo(Origin::User).unwrap();
    h.run_steps(5);
    assert_ne!(h.state().scene.ratsnest, initial);
}
