//! Inspection-only: actual egui test inputs prepare production ViamindApp;
//! a fixed native WGPU wrapper renders that state without OS pointer injection or MCP.
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before, "inspection must preserve full prepared session");
    }
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().pcb_import.busy() && std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(5);
    assert!(!h.state().pcb_import.busy());
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("PCB source path required")?;
    let mode = std::env::args().nth(2).unwrap_or_else(|| "preview".into());
    let width = std::env::args().nth(3).map_or(Ok(1600.0), |s| s.parse::<f32>())?;
    let height = std::env::args().nth(4).map_or(Ok(1000.0), |s| s.parse::<f32>())?;
    let engine = EngineHandle::new(viamind_doc::Project::new("Import inspection"));
    let original = engine.session_state();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_pcb_import();
    let mut h = Harness::builder().with_size(egui::vec2(width, height)).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(3);
    h.get_by_label("PCB source file").focus();
    h.run_steps(2);
    h.get_by_label("PCB source file").type_text(&path);
    h.run_steps(3);
    click(&mut h, "Preview PCB import");
    wait(&mut h);
    assert!(h.state().pcb_import.preview().expect("current preview").can_import);
    assert_eq!(engine.session_state(), original);
    if mode == "imported" {
        click(&mut h, "Apply PCB import as unsaved native document");
        wait(&mut h);
        assert_eq!(engine.read(|p| p.board.parts.len()), 1);
        assert_eq!(engine.read(|p| p.board.tracks.len()), 2);
        assert_eq!(engine.read(|p| p.board.vias.len()), 1);
        assert!(engine.document_status().dirty);
    }
    if mode == "compact" {
        h.get_by_label("Apply PCB import as unsaved native document").scroll_to_me();
        h.run_steps(8);
        let rect = h.get_by_label("Apply PCB import as unsaved native document").rect();
        assert!(rect.min.x >= 0.0 && rect.max.x <= width && rect.min.y >= 0.0 && rect.max.y <= height);
    }
    let memory = h.ctx.memory(Clone::clone);
    let before = engine.session_state();
    let mut app = if mode == "imported" { ViamindApp::with_design(engine.clone(), None) } else { h.into_state() };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind PCB import inspection")
            .with_app_id("viamind-pcb-import-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind PCB import inspection",
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
