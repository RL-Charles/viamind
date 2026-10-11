//! Real accessible controls and production output/state behavior.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_ui_egui::{ViamindApp, actions::UiAction};
fn engine() -> EngineHandle {
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    p.board = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap().board;
    EngineHandle::new(p)
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, value: &str) {
    h.get_by_label(label).focus();
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(2);
    h.event(Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(1);
    h.get_by_label(label).type_text(value);
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().view_export.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().view_export.busy());
}
#[test]
fn actual_file_menu_form_exports_all_formats_and_matches_shared_renderers_without_edits() {
    let e = engine();
    let before = e.session_state();
    let status = e.document_status();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registry = viamind_mcp::engine_registry(e.clone()).unwrap();
    let app = ViamindApp::with_design(e.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    click(&mut h, "File");
    click(&mut h, "Export rendered view…");
    let directory = tempfile::tempdir().unwrap();
    input(&mut h, "New output file", directory.path().join("bad.png").to_str().unwrap());
    click(&mut h, "Export snapshot to NEW file");
    assert!(h.query_by_label("Output path must end in .svg for the selected format.").is_some());
    assert!(!h.state().view_export.busy());
    for schematic in [false, true] {
        if schematic {
            click(&mut h, "SVG");
            click(&mut h, "Schematic sheet");
            input(&mut h, "New output file", directory.path().join("no-sheet.svg").to_str().unwrap());
            click(&mut h, "Export snapshot to NEW file");
            assert!(h.query_by_label("Select the schematic sheet to export.").is_some());
            click(&mut h, "Schematic sheet to export");
            let label = e.read(|p| {
                let s = p.schematic.sheets.values().next().unwrap();
                format!("{} ({})", s.name, s.id)
            });
            click(&mut h, &label);
        }
        for png in [false, true] {
            click(&mut h, if png { "PNG" } else { "SVG" });
            let path = directory.path().join(format!("{}-view.{}", if schematic { "schematic" } else { "board" }, if png { "png" } else { "svg" }));
            input(&mut h, "New output file", path.to_str().unwrap());
            click(&mut h, "Export snapshot to NEW file");
            wait(&mut h);
            let bytes = std::fs::read(&path).unwrap();
            let expected = e.read(|p| {
                if schematic {
                    let snapshot = viamind_render::schematic_view_snapshot(p, *p.schematic.sheets.keys().next().unwrap()).unwrap();
                    if png {
                        viamind_render::schematic_png(&snapshot, &Default::default()).unwrap().png
                    } else {
                        viamind_render::schematic_svg(&snapshot, &Default::default()).unwrap().svg.into_bytes()
                    }
                } else if png {
                    viamind_render::board_png(p, &Default::default()).unwrap().png
                } else {
                    viamind_render::board_svg(p, &Default::default()).unwrap().svg.into_bytes()
                }
            });
            assert_eq!(bytes, expected);
            let mcp_path =
                directory.path().join(format!("mcp-{}-view.{}", if schematic { "schematic" } else { "board" }, if png { "png" } else { "svg" }));
            let mut args = json!({"path":mcp_path,"expected_cursor":before.cursor});
            if schematic {
                args["sheet"] = json!(e.read(|p| *p.schematic.sheets.keys().next().unwrap()));
            }
            let action = match (schematic, png) {
                (false, false) => UiAction::RenderBoardSvg,
                (false, true) => UiAction::RenderBoardPng,
                (true, false) => UiAction::RenderSchematicSvg,
                (true, true) => UiAction::RenderSchematicPng,
            };
            runtime.block_on(registry.call(action.tool(), args, viamind_mcp::agent::CallContext::new("desktop-output-comparison"))).unwrap();
            assert_eq!(std::fs::read(mcp_path).unwrap(), bytes);

            assert!(h.query_by_label(&format!("Exported {}", path.display())).is_some());
            click(&mut h, "Export snapshot to NEW file");
            wait(&mut h);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert!(h.query_all_by_label_contains("Export failed:").next().is_some());
        }
    }
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    input(&mut h, "New output file", directory.path().join("source-snapshot.png").to_str().unwrap());
    click(&mut h, "Export snapshot to NEW file");
    wait(&mut h);
    e.execute("add_part", json!({"designator":"R999","value":"","footprint":"R_0603","x_mm":0,"y_mm":0}), Origin::User).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("The design has changed since this exported snapshot.").is_some());
}
#[test]
fn cropped_layers_dimensions_and_decimal_padding_are_validated_in_the_form() {
    let e = engine();
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_view_export();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1400.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(3);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crop.svg");
    input(&mut h, "New output file", path.to_str().unwrap());
    input(&mut h, "Width (px)", "0");
    click(&mut h, "Export snapshot to NEW file");
    assert!(!path.exists());
    input(&mut h, "Width (px)", "320");
    input(&mut h, "Height (px)", "200");
    click(&mut h, "Top copper");
    click(&mut h, "Bottom copper");
    input(&mut h, "Auto-fit padding (mm)", "0.0000001");
    click(&mut h, "Export snapshot to NEW file");
    assert!(!path.exists());
    input(&mut h, "Auto-fit padding (mm)", "0.000001");
    click(&mut h, "Explicit crop (mm, X right / Y down)");
    input(&mut h, "Minimum X (mm)", "-0.000001");
    input(&mut h, "Maximum X (mm)", "30.000001");
    click(&mut h, "Export snapshot to NEW file");
    wait(&mut h);
    let expected = e.read(|p| {
        viamind_render::board_svg(
            p,
            &viamind_render::SvgOptions {
                layers: vec![viamind_render::ViewLayer::Outline],
                width_px: 320,
                height_px: 200,
                padding_nm: 1,
                region_nm: Some(viamind_render::ViewBounds { min_x: -1, min_y: 0, max_x: 30_000_001, max_y: 20_000_000 }),
            },
        )
        .unwrap()
        .svg
    });
    assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
}
#[test]
fn catalog_actions_reject_stale_cursor_missing_sheet_and_native_destination_atomically() {
    let e = engine();
    let old = e.event_cursor();
    e.execute("add_part", json!({"designator":"R99","value":"","footprint":"R_0603","x_mm":0,"y_mm":0}), Origin::User).unwrap();
    let before = e.session_state();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stale.svg");
    assert!(UiAction::RenderBoardSvg.run(&e, json!({"path":path,"expected_cursor":old})).unwrap_err().to_string().contains("changed before"));
    assert!(!path.exists());
    assert!(UiAction::RenderSchematicSvg.run(&e, json!({"sheet":viamind_doc::ObjectId::new(),"path":path})).is_err());
    assert!(!path.exists());
    assert_eq!(e.session_state(), before);
    let native = dir.path().join("project.svg");
    let fixture = include_str!("../../../fixtures/views/asymmetric.viamind.json");
    std::fs::write(&native, fixture).unwrap();
    let e = EngineHandle::load(&native).unwrap();
    let status = e.document_status();
    let before = e.session_state();
    assert!(UiAction::RenderBoardSvg.run(&e, json!({"path":native})).unwrap_err().to_string().contains("active native"));
    assert_eq!(std::fs::read_to_string(native).unwrap(), fixture);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
