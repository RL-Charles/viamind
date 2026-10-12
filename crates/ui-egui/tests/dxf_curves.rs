//! Actual compact 640x400 input for certified DXF preview, guarded Apply, stale and reopen paths.
use egui::{Event, Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::ViamindApp;

const ROUNDED: &str = include_str!("../../doc/tests/fixtures/outline-rounded.dxf");

fn reveal(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(10);
    let r = h.get_by_label(label).rect();
    h.hover_at(r.center());
    h.run_steps(2);
    let r = h.get_by_label(label).rect();
    let clip = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| {
            let matching = match &s.shape {
                egui::Shape::Text(t) => t.galley.text() == label,
                egui::Shape::Rect(b) => {
                    b.rect.contains(r.center()) && (b.rect.width() - r.width()).abs() <= 4. && (b.rect.height() - r.height()).abs() <= 4.
                }
                _ => false,
            };
            matching.then_some(s.clip_rect.intersect(h.ctx.content_rect()))
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or_else(|| panic!("missing paint for {label}"));
    assert!(clip.contains_rect(r), "{label} rect{r:?}, effectiveclip{clip:?}");
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    reveal(h, label);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    reveal(h, label);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(3);
}
fn settle(h: &mut Harness<'_, ViamindApp>) {
    for _ in 0..500 {
        h.run_steps(1);
        if !h.state().outlines.dxf_busy() {
            h.run_steps(4);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("DXF worker did not finish");
}
fn open_form(h: &mut Harness<'_, ViamindApp>) {
    h.state_mut().begin_board_outline();
    h.run_steps(5);
    if h.query_by_label("Preview curved DXF").is_none() {
        click(h, "Import ASCII DXF");
    }
}

#[test]
fn compact_actual_input_previews_applies_undoes_and_refuses_stale_or_changed_sources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rounded.dxf");
    std::fs::write(&path, ROUNDED).unwrap();
    let engine = EngineHandle::new(viamind_doc::Project::new("Compact DXF"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(640., 400.))
        .build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    open_form(&mut h);
    input(&mut h, "DXF file path", path.to_str().unwrap());
    input(&mut h, "DXF layer (empty = all)", "Edge");
    input(&mut h, "Curve tolerance (nm)", "10000");
    for label in ["Preview curved DXF", "Apply curved DXF outline", "Reset DXF form", "Apply outline", "Cancel"] {
        reveal(&mut h, label);
    }
    let before = engine.session_state();
    // Apply is disabled without a fresh preview.
    click(&mut h, "Apply curved DXF outline");
    settle(&mut h);
    assert_eq!(engine.session_state(), before);
    click(&mut h, "Preview curved DXF");
    settle(&mut h);
    assert_eq!(engine.session_state(), before, "preview never mutates");
    let preview = h.state().outlines.dxf_preview().cloned().expect("fresh preview");
    assert_eq!(preview.outline.cutouts.len(), 2);
    let summary = format!(
        "Polygon approximation: {} vertices, {} cutouts; certified bound <= {} nm (requested 10000 nm).",
        preview.report.vertices, preview.report.cutouts, preview.report.achieved_bound_nm
    );
    reveal(&mut h, &summary);

    // Source rewrite after preview: Apply refuses by digest and reports it.
    std::fs::write(&path, ROUNDED.replace("40\n1.6", "40\n1.5")).unwrap();
    click(&mut h, "Apply curved DXF outline");
    settle(&mut h);
    assert_eq!(engine.session_state(), before);
    assert!(h.query_by_label_contains("Apply failed; nothing changed").is_some());
    std::fs::write(&path, ROUNDED).unwrap();

    // A peer edit makes the next preview stale; Apply stays disabled until a new preview.
    click(&mut h, "Preview curved DXF");
    settle(&mut h);
    engine.execute("add_part", json!({"designator":"P1","value":"peer","footprint":"X","x_mm":1,"y_mm":1}), Origin::agent("peer")).unwrap();
    h.run_steps(5);
    let peer = engine.session_state();
    click(&mut h, "Apply curved DXF outline");
    settle(&mut h);
    assert_eq!(engine.session_state(), peer);
    assert!(h.state().outlines.dxf_preview().is_none());

    click(&mut h, "Preview curved DXF");
    settle(&mut h);
    click(&mut h, "Apply curved DXF outline");
    settle(&mut h);
    let after = engine.session_state();
    assert_eq!(after.project.board.outline.as_ref(), Some(&preview.outline));
    assert_eq!(after.cursor.sequence, peer.cursor.sequence + 1);
    assert!(h.query_by_label_contains("Applied curved DXF outline as one undoable change").is_some());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), ROUNDED);

    // Close and reopen keep the local form; Undo restores the previous outline.
    click(&mut h, "Cancel");
    open_form(&mut h);
    assert_eq!(h.get_by_label("DXF file path").value().as_deref(), Some(path.to_str().unwrap()));
    click(&mut h, "Cancel");
    h.state_mut().undo_design();
    h.run_steps(5);
    assert!(engine.read(|p| p.board.outline.is_none()));
    assert!(engine.read(|p| p.board.part_by_designator("P1").is_some()));
}
