//! Actual egui input prepares production ViamindApp, then a fixed native WGPU
//! wrapper renders it. No OS pointer injection or alternate command implementation.
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, SessionSnapshot};
use viamind_ui_egui::{ViamindApp, actions::UiAction};
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before, "inspection preserves complete native session");
    }
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(5);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().footprint_conversion.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(5);
    assert!(!h.state().footprint_conversion.busy());
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args().nth(1).ok_or("source required")?;
    let output = std::env::args().nth(2).ok_or("new output required")?;
    let mode = std::env::args().nth(3).unwrap_or_else(|| "preview".into());
    let width = std::env::args().nth(4).map_or(Ok(1600.0), |s| s.parse::<f32>())?;
    let height = std::env::args().nth(5).map_or(Ok(1000.0), |s| s.parse::<f32>())?;
    let engine = EngineHandle::new(viamind_doc::Project::new("Detached conversion inspection"));
    UiAction::PlacePart
        .run(&engine, serde_json::json!({"designator":"KEEP1","value":"dirty native work preserved","footprint":"Original","x_mm":0,"y_mm":0}))?;
    let before = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_footprint_conversion();
    let mut h = Harness::builder().with_size(egui::vec2(width, height)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(3);
    for (label, text) in [("Footprint source file", source.as_str()), ("New native footprint file", output.as_str())] {
        h.get_by_label(label).scroll_to_me();
        h.run_steps(5);
        h.get_by_label(label).focus();
        h.run_steps(2);
        h.get_by_label(label).type_text(text);
        h.run_steps(3);
    }
    click(&mut h, "Preview footprint conversion");
    wait(&mut h);
    assert!(h.state().footprint_conversion.preview().unwrap().can_convert);
    assert_eq!(engine.session_state(), before);
    if mode == "completed" {
        click(&mut h, "Create new native footprint file");
        wait(&mut h);
        assert_eq!(viamind_doc::footprint::load(std::path::Path::new(&output))?.pads.len(), 2);
    } else if mode != "compact" {
        click(&mut h, "Footprint per-list fidelity report");
    }
    if mode == "normalization" {
        let label = "source angle 90 degrees, canonical 90 degrees";
        let header = h.get_by_label("Footprint per-list fidelity report").rect();
        let inside_report = egui::pos2(header.center().x, header.max.y + 120.0);
        assert!(inside_report.y < height, "report viewport must fit inspection window");
        for _ in 0..150 {
            if h.query_by_label_contains(label).is_some() {
                break;
            }
            // Keep the pointer inside the report viewport; virtual rows can be clipped.
            h.event(egui::Event::PointerMoved(inside_report));
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -100.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(8);
        }
        h.get_by_label_contains(label).scroll_to_me();
        h.run_steps(8);
        let rect = h.get_by_label_contains(label).rect();
        assert!(rect.min.y >= 0.0 && rect.max.y <= height, "normalization row must be visible");
    }
    if mode == "compact" {
        h.get_by_label("Create new native footprint file").scroll_to_me();
        h.run_steps(8);
        let rect = h.get_by_label("Create new native footprint file").rect();
        assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= width && rect.max.y <= height);
    }
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind footprint conversion inspection")
            .with_app_id("viamind-footprint-conversion-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind footprint conversion inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
