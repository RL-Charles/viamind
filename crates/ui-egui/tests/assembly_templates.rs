//! Real compact input and MCP byte parity for both populated-only templates.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_engine::EngineHandle;
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
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
#[test]
fn compact_template_choices_export_counts_and_errors_match_mcp() {
    let project = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/bom-templates.viamind.json")).unwrap();
    let engine = EngineHandle::new(project);
    let before = engine.session_state();
    let mut h = Harness::builder()
        .with_size(vec2(640., 400.))
        .build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), ViamindApp::with_design(engine.clone(), None));
    h.run_steps(5);
    click(&mut h, "File");
    click(&mut h, "Export assembly CSV…");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(engine.clone()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for (name, label) in [("catalog_bom", "Catalog BOM"), ("assembly_bom", "Assembly BOM")] {
        click(&mut h, label);
        let desktop = temp.path().join(format!("desktop-{name}.csv"));
        let mcp = temp.path().join(format!("mcp-{name}.csv"));
        input(&mut h, "CSV output file", desktop.to_str().unwrap());
        click(&mut h, "Export CSV to NEW file");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while h.state().assembly_export.busy() && std::time::Instant::now() < deadline {
            h.run_steps(3);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.run_steps(3);
        assert!(!h.state().assembly_export.busy());
        let result = runtime
            .block_on(registry.call(
                "export_assembly_csv",
                json!({"kind":name,"path":mcp,"expected_cursor":engine.event_cursor()}),
                Default::default(),
            ))
            .unwrap();
        assert_eq!(result["component_count"], 1);
        assert_eq!(std::fs::read(&desktop).unwrap(), std::fs::read(mcp).unwrap());
        h.get_by_label(format!("1 components · {label}").as_str()).scroll_to_me();
        h.run_steps(5);
        assert!(
            h.get_by_label(format!("1 components · {label}").as_str())
                .rect()
                .intersects(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(640., 400.)))
        );
        click(&mut h, "Export CSV to NEW file");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while h.state().assembly_export.busy() && std::time::Instant::now() < deadline {
            h.run_steps(3);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.run_steps(3);
        assert!(!h.state().assembly_export.busy());
        assert!(desktop.exists());
        let error = h.get_by_label_contains("Export failed:");
        error.scroll_to_me();
        h.run_steps(5);
        assert!(h.get_by_label_contains("Export failed:").rect().max.y <= 400.0);
    }
    assert_eq!(engine.session_state(), before);
}
