use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_doc::{
    ObjectId, Project, Sheet,
    schematic::{Coordinate, PinType, SchematicContent, SchematicSymbol, SchematicWire, SymbolPin},
};
use viamind_engine::{EngineHandle, Origin};
use viamind_geom::Nm;
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
fn p(x: i64, y: i64) -> Coordinate {
    Coordinate { x: Nm(x), y: Nm(y) }
}
fn fixture() -> (EngineHandle, [Coordinate; 2]) {
    let mut project = Project::new("Interactive wires");
    let mut sheet = Sheet::new("Wire drawing");
    let mut c = SchematicContent::default();
    let mut endpoints = vec![];
    for (n, at) in [p(4_000_001, 4_000_003), p(16_000_007, 12_000_011)].into_iter().enumerate() {
        let s = SchematicSymbol {
            id: ObjectId::new(),
            designator: format!("U{}", n + 1),
            value: "Original".into(),
            footprint: None,
            position: at,
            rotation_quarters: 1,
            mirrored: true,
            width: Nm(2_000_000),
            height: Nm(2_000_000),
            pins: vec![SymbolPin {
                id: ObjectId::new(),
                number: "1".into(),
                name: "SIG".into(),
                position: p(-2_000_013, 257),
                electrical: PinType::Passive,
            }],
        };
        endpoints.push(s.pin_position(s.pins[0].position).unwrap());
        c.symbols.insert(s.id, s);
    }
    sheet.content = Some(c);
    project.schematic.sheets.insert(sheet.id, sheet);
    (EngineHandle::new(project), [endpoints[0], endpoints[1]])
}
fn harness(e: &EngineHandle) -> Harness<'static, ViamindApp> {
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.set_schematic_mode(true);
    let mut h = Harness::builder().with_size(vec2(1600.0, 2000.0)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(5);
    h
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn begin(h: &mut Harness<'_, ViamindApp>) {
    click(h, "Draw wires on canvas");
    for _ in 0..300 {
        h.run_steps(2);
        if h.query_by_label("0 confirmed vertices").is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("wire snapshot missing");
}
fn screen(h: &Harness<'_, ViamindApp>, at: Coordinate) -> egui::Pos2 {
    h.state().schematic.camera.world_to_screen(WorldPos::new(at.x.to_mm(), at.y.to_mm()), h.state().schematic.viewport)
}
fn pointer(h: &mut Harness<'_, ViamindApp>, at: Coordinate, offset: egui::Vec2) {
    let pos = screen(h, at) + offset;
    for pressed in [true, false] {
        h.event(Event::PointerMoved(pos));
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(3);
}
fn key(h: &mut Harness<'_, ViamindApp>, key: Key) {
    for pressed in [true, false] {
        h.event(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    h.run_steps(3);
}
#[test]
fn actual_pointer_exact_rotated_mirrored_offgrid_attachment_finish_and_undo() {
    let (e, ends) = fixture();
    let before = e.session_state();
    let mut h = harness(&e);
    begin(&mut h);
    pointer(&mut h, ends[0], vec2(5.0, 0.0));
    pointer(&mut h, ends[1], vec2(-4.0, 0.0));
    let points = h.state().schematic_wires.confirmed_vertices();
    assert_eq!(points.first(), Some(&ends[0]));
    assert_eq!(points.last(), Some(&ends[1]));
    assert_eq!(points.len(), 3);
    assert_eq!(e.session_state(), before);
    key(&mut h, Key::Enter);
    assert!(!h.state().schematic_wires.active());
    let after = e.read(Clone::clone);
    let c = after.schematic.sheets.values().next().unwrap().content.as_ref().unwrap();
    assert_eq!(c.wires.len(), 2);
    let graph = e.query_schematic_connectivity(&Default::default()).unwrap();
    assert_eq!(graph.nets.len(), 1);
    assert_eq!(graph.nets[0].members.iter().filter(|m| m.kind == viamind_engine::SchematicMemberKind::Pin).count(), 2);
    h.state_mut().undo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), before.project);
    h.state_mut().redo_design();
    h.run_steps(5);
    assert_eq!(e.read(Clone::clone), after);
}
#[test]
fn actual_posture_backspace_pan_zoom_and_escape_keep_entire_model_unchanged() {
    let (e, ends) = fixture();
    let before = e.session_state();
    let mut h = harness(&e);
    begin(&mut h);
    click(&mut h, "Horizontal first");
    pointer(&mut h, ends[0], vec2(0.0, 0.0));
    pointer(&mut h, ends[1], vec2(0.0, 0.0));
    let points = h.state().schematic_wires.confirmed_vertices();
    assert_eq!(points[1], Coordinate { x: ends[0].x, y: ends[1].y });
    key(&mut h, Key::Backspace);
    assert_eq!(h.state().schematic_wires.confirmed_vertices(), vec![ends[0]]);
    let pos = h.state().schematic.viewport.center();
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton { pos, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerMoved(pos + vec2(35.0, 20.0)));
    h.run_steps(3);
    h.event(Event::PointerButton { pos: pos + vec2(35.0, 20.0), button: PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 30.0), modifiers: Modifiers::NONE });
    h.run_steps(5);
    assert_eq!(h.state().schematic_wires.confirmed_vertices(), vec![ends[0]]);
    key(&mut h, Key::Escape);
    assert!(!h.state().schematic_wires.active());
    assert_eq!(e.session_state(), before);
}
#[test]
fn actual_stale_peer_edit_reopen_and_object_draft_guards_preserve_content() {
    let (e, ends) = fixture();
    let mut h = harness(&e);
    begin(&mut h);
    pointer(&mut h, ends[0], vec2(0.0, 0.0));
    pointer(&mut h, ends[1], vec2(0.0, 0.0));
    e.execute("add_part", serde_json::json!({"designator":"R99","value":"peer","footprint":"x","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    h.run_steps(5);
    let before = e.session_state();
    click(&mut h, "Finish wires");
    assert_eq!(e.session_state(), before);
    assert!(h.state().schematic_wires.active());
    assert!(h.query_by_label_contains("local preview retained").is_some());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    h.run_steps(5);
    assert!(!h.state().schematic_wires.active());
    click(&mut h, "Add wire");
    let before = e.session_state();
    click(&mut h, "Draw wires on canvas");
    assert!(!h.state().schematic_wires.active());
    assert!(h.query_by_label_contains("Finish or cancel the current object").is_some());
    assert_eq!(before, e.session_state());
}
#[test]
fn actual_crossing_does_not_join_and_grid_t_endpoint_does() {
    let (e, _) = fixture();
    let mut project = e.read(Clone::clone);
    let c = project.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap();
    c.symbols.clear();
    let id = ObjectId::new();
    c.wires.insert(id, SchematicWire { id, start: p(10_000_000, 0), end: p(10_000_000, 20_000_000) });
    let e = EngineHandle::new(project);
    let mut h = harness(&e);
    begin(&mut h);
    pointer(&mut h, p(5_000_000, 10_000_000), vec2(0.0, 0.0));
    pointer(&mut h, p(15_000_000, 10_000_000), vec2(0.0, 0.0));
    key(&mut h, Key::Enter);
    assert_eq!(e.query_schematic_connectivity(&Default::default()).unwrap().nets.len(), 2);
    begin(&mut h);
    pointer(&mut h, p(12_000_000, 4_000_000), vec2(0.0, 0.0));
    pointer(&mut h, p(10_000_000, 4_000_000), vec2(0.0, 0.0));
    key(&mut h, Key::Enter);
    let graph = e.query_schematic_connectivity(&Default::default()).unwrap();
    assert_eq!(graph.nets.len(), 2);
    assert_eq!(graph.nets.iter().map(|n| n.members.len()).max(), Some(2));
}
