//! Real pointer/keyboard gestures against the app and its shared engine.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use viamind_doc::{ObjectId, Project};
use viamind_engine::{EngineHandle, Origin, SelectionMode, SetSelection};
use viamind_ui_egui::{ViamindApp, camera::WorldPos, layers::LayerId, selection::SelectionTool};

fn setup() -> (EngineHandle, Vec<ObjectId>, Harness<'static, ViamindApp>) {
    let engine = EngineHandle::new(Project::new("selection"));
    let ids = [20, 30, 45]
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            engine
                .execute(
                    "add_part",
                    json!({ "designator": format!("R{}", i+1), "value": "10k", "footprint": "R_0603", "x_mm": x, "y_mm": 30 }),
                    Origin::User,
                )
                .unwrap()
                .created[0]
        })
        .collect();
    let mut harness = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    harness.run_steps(2);
    (engine, ids, harness)
}

fn screen(h: &Harness<'_, ViamindApp>, x: f64, y: f64) -> Pos2 {
    h.state().canvas.camera.world_to_screen(WorldPos::new(x, y), h.state().canvas.viewport)
}

fn button(h: &mut Harness<'_, ViamindApp>, p: Pos2, pressed: bool, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerMoved(p));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers });
    h.run_steps(1);
}

fn click(h: &mut Harness<'_, ViamindApp>, x: f64, y: f64, modifiers: Modifiers) {
    let p = screen(h, x, y);
    button(h, p, true, modifiers);
    button(h, p, false, modifiers);
}

fn drag(h: &mut Harness<'_, ViamindApp>, points: &[(f64, f64)], modifiers: Modifiers) {
    let start = screen(h, points[0].0, points[0].1);
    button(h, start, true, modifiers);
    for &(x, y) in &points[1..] {
        let p = screen(h, x, y);
        h.event(Event::PointerMoved(p));
        h.run_steps(1);
    }
    let &(x, y) = points.last().unwrap();
    let end = screen(h, x, y);
    button(h, end, false, modifiers);
}

fn sorted(mut ids: Vec<ObjectId>) -> Vec<ObjectId> {
    ids.sort();
    ids
}

#[test]
fn click_modifiers_blank_click_and_agent_selection_share_the_same_state() {
    let (engine, ids, mut h) = setup();
    click(&mut h, 20.0, 30.0, Modifiers::NONE);
    assert_eq!(engine.selection().ids, [ids[0]]);
    click(&mut h, 30.0, 30.0, Modifiers::SHIFT);
    assert_eq!(engine.selection().ids, sorted(vec![ids[0], ids[1]]));
    click(&mut h, 20.0, 30.0, Modifiers::CTRL);
    assert_eq!(engine.selection().ids, [ids[1]]);
    click(&mut h, 30.0, 30.0, Modifiers::CTRL.plus(Modifiers::SHIFT));
    assert!(engine.selection().ids.is_empty());
    let handle = engine.clone();
    let id = ids[2];
    std::thread::spawn(move || handle.set_selection(SetSelection { ids: vec![id], mode: SelectionMode::Replace }, Origin::agent("tester")))
        .join()
        .unwrap()
        .unwrap();
    h.run_steps(1);
    assert_eq!(h.state().design.as_ref().unwrap().engine.selection().ids, [ids[2]]);
    h.get_by_label("3 parts · 1 selected");
    click(&mut h, 70.0, 50.0, Modifiers::NONE);
    assert!(engine.selection().ids.is_empty());
}

#[test]
fn box_direction_lasso_and_hidden_layers() {
    let (engine, ids, mut h) = setup();
    drag(&mut h, &[(17.0, 28.0), (32.0, 32.0)], Modifiers::NONE);
    assert_eq!(engine.selection().ids, sorted(vec![ids[0], ids[1]]));
    // Left-to-right encloses neither full part; right-to-left touches both.
    drag(&mut h, &[(20.0, 28.0), (30.0, 32.0)], Modifiers::NONE);
    assert!(engine.selection().ids.is_empty());
    drag(&mut h, &[(30.0, 28.0), (20.0, 32.0)], Modifiers::NONE);
    assert_eq!(engine.selection().ids, sorted(vec![ids[0], ids[1]]));
    h.state_mut().selection.tool = SelectionTool::Lasso;
    drag(&mut h, &[(42.0, 27.0), (48.0, 27.0), (48.0, 33.0), (42.0, 33.0), (42.0, 27.0)], Modifiers::NONE);
    assert_eq!(engine.selection().ids, [ids[2]]);
    for layer in [LayerId::F_CU, LayerId::F_MASK, LayerId::F_SILKS] {
        h.state_mut().layers.set_visible(layer, false);
    }
    h.run_steps(1);
    click(&mut h, 45.0, 30.0, Modifiers::NONE);
    assert!(engine.selection().ids.is_empty(), "entirely hidden part is not pickable");
}

#[test]
fn pan_escape_and_select_all() {
    let (engine, ids, mut h) = setup();
    click(&mut h, 20.0, 30.0, Modifiers::NONE);
    let start = screen(&h, 17.0, 28.0);
    button(&mut h, start, true, Modifiers::NONE);
    let end = screen(&h, 48.0, 33.0);
    h.event(Event::PointerMoved(end));
    h.run_steps(1);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    button(&mut h, end, false, Modifiers::NONE);
    assert_eq!(engine.selection().ids, [ids[0]], "Escape cancels pending drag");
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    assert_eq!(engine.selection().ids, sorted(ids));
    let before = engine.selection();
    h.event(Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    drag(&mut h, &[(17.0, 28.0), (32.0, 32.0)], Modifiers::NONE);
    h.event(Event::Key { key: Key::Space, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(engine.selection(), before, "panning preserves selection");
}

#[test]
fn lasso_refines_bounding_box_candidates_and_detects_edge_crossings() {
    use viamind_geom::{Point, Polygon};
    let (_engine, ids, h) = setup();
    let ring = |points: &[(f64, f64)]| Polygon::new(points.iter().map(|&(x, y)| Point::from_mm(x, y)).collect());
    let triangle = ring(&[(17.0, 27.0), (48.0, 27.0), (17.0, 40.0)]);
    assert_eq!(
        sorted(h.state().selection.lasso_ids(&triangle)),
        sorted(vec![ids[0], ids[1]]),
        "R3 is inside the broad-phase bounds but outside the lasso"
    );
    let inside = ring(&[(44.9, 29.9), (45.1, 29.9), (45.1, 30.1), (44.9, 30.1)]);
    assert_eq!(h.state().selection.lasso_ids(&inside), [ids[2]]);
    let crossing = ring(&[(10.0, 29.9), (55.0, 29.9), (55.0, 30.1), (10.0, 30.1)]);
    assert_eq!(sorted(h.state().selection.lasso_ids(&crossing)), sorted(ids));
}
