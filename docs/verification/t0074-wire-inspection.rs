//! Inspection-only native harness. Injects egui pointer events through raw_input_hook;
//! the unchanged production canvas handles actual click/preview state.
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
struct Inspect {
    app: ViamindApp,
    frames: u32,
    stage: u8,
    first: WorldPos,
    second: WorldPos,
    engine: viamind_engine::EngineHandle,
    before: viamind_doc::Project,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        self.frames += 1;
        if self.frames == 8 {
            self.app.begin_schematic_wires();
        }
        if self.stage >= 4 {
            assert_eq!(self.engine.read(Clone::clone), self.before, "native local preview must not mutate design");
            let points = self.app.schematic_wires.confirmed_vertices();
            assert_eq!(points.len(), 3, "two clicks must produce one cardinal bend");
        }
        if self.frames < 80 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.frames < 20 || !self.app.schematic_wires.previewing() || self.stage >= 4 {
            return;
        }
        let at = if self.stage < 2 { self.first } else { self.second };
        let pos = self.app.schematic.camera.world_to_screen(at, self.app.schematic.viewport);
        raw.events.push(egui::Event::PointerMoved(pos));
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: self.stage.is_multiple_of(2),
            modifiers: egui::Modifiers::NONE,
        });
        self.stage += 1;
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = viamind_doc::file::load(std::env::args_os().nth(1).ok_or("project path required")?)?;
    let symbol = project
        .schematic
        .sheets
        .values()
        .flat_map(|s| s.content.iter().flat_map(|c| c.symbols.values()))
        .find(|s| s.designator == "U1")
        .ok_or("U1 required")?;
    let pin = symbol.pins.iter().find(|p| p.number == "1").ok_or("pin1 required")?;
    let first = symbol.pin_position(pin.position)?;
    let first = WorldPos::new(first.x.to_mm(), first.y.to_mm());
    let second = WorldPos::new(first.x + 4.0, first.y - 5.0);
    let engine = viamind_engine::EngineHandle::new(project.clone());
    let width = std::env::args().nth(2).map_or(Ok(1600.0), |n| n.parse::<f32>())?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind schematic wire inspection")
            .with_app_id("viamind-wire-inspection")
            .with_inner_size([width, 1000.0])
            .with_min_inner_size([width, 1000.0])
            .with_max_inner_size([width, 1000.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind schematic wire inspection",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_design(engine.clone(), None);
            app.configure_renderer(cc);
            app.set_schematic_mode(true);
            Ok(Box::new(Inspect { app, frames: 0, stage: 0, first, second, engine, before: project }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
