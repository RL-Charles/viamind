use egui::{Color32, Event, Key, Modifiers, Shape, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, PresencePoint, SetAgentPresence};
use viamind_ui_egui::ViamindApp;

fn text_shapes<'a>(shape: &'a Shape, found: &mut Vec<&'a egui::epaint::TextShape>) {
    match shape {
        Shape::Text(text) => found.push(text),
        Shape::Vec(shapes) => {
            for shape in shapes {
                text_shapes(shape, found);
            }
        }
        _ => {}
    }
}

#[test]
fn compact_colored_attention_is_clipped_and_human_edit_undo_stays_live() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(project.clone());
    let before = engine.session_state();
    let status = engine.document_status();
    for (index, part) in project.board.parts.values().take(2).enumerate() {
        engine
            .set_agent_presence(
                index as u64 + 1,
                ["Routing agent", "Assembly agent"][index],
                SetAgentPresence {
                    surface: viamind_engine::PresenceSurface::Pcb {},
                    expected_project: project.id,
                    expected_stream: before.cursor.stream_id,
                    cursor: PresencePoint { x: part.position.x, y: part.position.y },
                    selection: vec![part.id],
                    working_on: ["Checking via clearance", "Calibrating centroid"][index].into(),
                },
            )
            .unwrap();
    }
    assert_eq!(engine.query_agent_presence().agents.len(), 2);
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    let mut seen = Vec::new();
    for clipped in &h.output().shapes {
        let mut texts = Vec::new();
        text_shapes(&clipped.shape, &mut texts);
        for text in texts {
            if text.galley.job.text.contains("agent #") {
                assert!(clipped.clip_rect.contains(text.pos));
                assert!(clipped.clip_rect.contains(text.pos + text.galley.size()));
                seen.push((text.galley.job.text.clone(), text.fallback_color));
            }
        }
    }
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_ne!(seen[0].1, seen[1].1);
    assert!(seen.iter().all(|(_, color)| *color != Color32::TRANSPARENT));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    h.get_by_label("Edit").click();
    h.run_steps(4);
    h.get_by_label("Place test resistor").click();
    h.run_steps(5);
    assert_eq!(engine.read(|p| p.board.parts.len()), project.board.parts.len() + 1);
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::Z, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(3);
    }
    assert_eq!(engine.read(Clone::clone), project);
    assert_eq!(engine.query_agent_presence().agents.len(), 2);
}

fn attention_labels(h: &Harness<'_, ViamindApp>) -> Vec<String> {
    let mut seen = Vec::new();
    for clipped in &h.output().shapes {
        let mut texts = Vec::new();
        text_shapes(&clipped.shape, &mut texts);
        for text in texts {
            if text.galley.job.text.contains("agent #") {
                assert!(clipped.clip_rect.contains(text.pos));
                assert!(clipped.clip_rect.contains(text.pos + text.galley.size()));
                seen.push(text.galley.job.text.clone());
            }
        }
    }
    seen
}

#[test]
fn schematic_attention_filters_views_and_sheets_draws_exact_bounds_and_preserves_human_undo() {
    use viamind_engine::PresenceSurface;
    use viamind_geom::Nm;
    use viamind_ui_egui::camera::WorldPos;
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    project.board = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap().board;
    let sheet = project.schematic.sheets.values().next().unwrap().clone();
    let content = sheet.content.as_ref().unwrap();
    let symbol = content.symbols.values().next().unwrap();
    let empty = viamind_doc::Sheet { id: viamind_doc::ObjectId::new(), name: "Empty attention".into(), content: None };
    project.schematic.sheets.insert(empty.id, empty.clone());
    let engine = EngineHandle::new(project.clone());
    let before = engine.session_state();
    let status = engine.document_status();
    let selected =
        vec![symbol.id, *content.wires.keys().next().unwrap(), *content.junctions.keys().next().unwrap(), *content.labels.keys().next().unwrap()];
    let part = project.board.parts.values().next().unwrap();
    for (connection, name, surface, cursor, selection) in [
        (1, "pcb agent", PresenceSurface::Pcb {}, PresencePoint { x: part.position.x, y: part.position.y }, vec![part.id]),
        (
            2,
            "schematic agent",
            PresenceSurface::Schematic { sheet: sheet.id },
            PresencePoint { x: symbol.position.x, y: symbol.position.y },
            selected,
        ),
        (3, "empty agent", PresenceSurface::Schematic { sheet: empty.id }, PresencePoint { x: Nm(0), y: Nm(0) }, vec![]),
    ] {
        engine
            .set_agent_presence(
                connection,
                name,
                SetAgentPresence {
                    expected_project: project.id,
                    expected_stream: before.cursor.stream_id,
                    surface,
                    cursor,
                    selection,
                    working_on: "Checking explicit attention".into(),
                },
            )
            .unwrap();
    }
    let app = ViamindApp::with_design(engine.clone(), None);
    let mut h = Harness::builder().with_size(vec2(640., 400.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(5);
    assert_eq!(attention_labels(&h).len(), 1);
    assert!(attention_labels(&h)[0].starts_with("pcb agent #"));
    h.get_by_label("Schematic").click();
    h.run_steps(5);
    // Choose the explicit populated sheet, even if UUID order made the empty sheet initial.
    if h.query_by_label("Empty attention").is_some() {
        h.get_by_label("Empty attention").click();
        h.run_steps(3);
        h.get_by_label(&sheet.name).click();
        h.run_steps(5);
    }
    assert_eq!(attention_labels(&h).len(), 1);
    assert!(attention_labels(&h)[0].starts_with("schematic agent #"));
    let cam = h.state().schematic.camera;
    let canvas = h.state().schematic.viewport;
    let expected = egui::Rect::from_two_pos(
        cam.world_to_screen(WorldPos::new(6.999999, 8.0), canvas),
        cam.world_to_screen(WorldPos::new(13.000002, 12.000001), canvas),
    )
    .expand(4.0);
    let tint = Color32::from_rgb(190, 160, 255);
    let rects: Vec<_> = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Rect(r) if r.stroke.color == tint => Some(r.rect),
            _ => None,
        })
        .collect();
    assert!(rects.iter().any(|r| r.min.distance(expected.min) < 0.01 && r.max.distance(expected.max) < 0.01), "{rects:?} expected{expected:?}");
    let circles = h.output().shapes.iter().filter(|s| matches!(&s.shape,Shape::Circle(c) if c.stroke.color==tint && c.radius==7.0)).count();
    assert_eq!(circles, 2, "selected junction and label anchors");
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    h.get_by_label(&sheet.name).click();
    h.run_steps(3);
    h.get_by_label("Empty attention").click();
    h.run_steps(5);
    assert_eq!(attention_labels(&h).len(), 1);
    assert!(attention_labels(&h)[0].starts_with("empty agent #"));
    assert_eq!(engine.session_state(), before);
    // Return and exercise an actual human shared command with markers alive.
    h.get_by_label("Empty attention").click();
    h.run_steps(3);
    h.get_by_label(&sheet.name).click();
    h.run_steps(5);
    h.set_size(vec2(1600., 1000.));
    h.run_steps(5);
    h.get_by_label("Add junction").click();
    h.run_steps(5);
    h.get_by_label("Apply schematic object").click();
    h.run_steps(5);
    assert_eq!(engine.read(|p| p.schematic.sheets[&sheet.id].content.as_ref().unwrap().junctions.len()), content.junctions.len() + 1);
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::Z, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(3);
    }
    assert_eq!(engine.read(Clone::clone), project);
    assert_eq!(engine.query_agent_presence().agents.len(), 3);
    assert_eq!(attention_labels(&h).len(), 1);
    assert!(attention_labels(&h)[0].starts_with("schematic agent #"));
}
