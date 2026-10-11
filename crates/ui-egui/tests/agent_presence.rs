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
