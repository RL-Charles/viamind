//! Inspection-only: actual egui kittest button inputs prepare the production app;
//! the fixed native wrapper then renders that state through the production WGPU UI.
//! This is not native OS pointer injection or a production MCP binary.
use egui_kittest::{Harness, kittest::Queryable};
use viamind_engine::{EngineHandle, Origin, SelectionMode, SetSelection};
use viamind_ui_egui::ViamindApp;
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: viamind_engine::SessionSnapshot,
    terminal: (viamind_doc::ObjectId, viamind_doc::ObjectId),
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before, "local probe/navigation must preserve entire session");
        assert_eq!(self.app.pin_pad_probe.selected_terminal(), Some(self.terminal));
    }
}
fn settle(h: &mut Harness<'_, ViamindApp>, label: &str) {
    for _ in 0..300 {
        h.run_steps(2);
        if h.query_by_label(label).is_some() { return; }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("Missing production control: {label}");
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = viamind_doc::file::load(std::env::args_os().nth(1).ok_or("project path required")?)?;
    let engine = EngineHandle::new(project);
    let id = engine.read(|p| p.board.parts.values().find(|p| p.designator == "U1").expect("U1").id);
    engine.set_selection(SetSelection {ids:vec![id],mode:SelectionMode::Replace},Origin::agent("inspection-fixture"))?;
    let before = engine.session_state();
    let r = engine.query_linked_pin_pads(&viamind_engine::PinPadLinkQuery {component_id:id,offset:0,limit:None})?;
    let row = &r.terminals[0];
    let terminal = (row.pin_id.unwrap(),row.pad_id.unwrap());
    let caption = format!("{} {} · pin present / pad present",row.number,row.pin_name.as_deref().unwrap_or(""));
    let board = std::env::args().nth(2).as_deref()==Some("board");
    let width = std::env::args().nth(3).map_or(Ok(1600.0),|n|n.parse::<f32>())?;
    let mut h = Harness::builder().with_size(egui::vec2(width,2000.0)).build_ui_state(|ui,app:&mut ViamindApp|app.show(ui,None),ViamindApp::with_design(engine.clone(),None));
    settle(&mut h,"Inspect pins and pads");
    h.get_by_label("Inspect pins and pads").click();h.run_steps(5);
    settle(&mut h,"Show schematic pin");
    h.get_by_label(&caption).click();h.run_steps(5);
    h.get_by_label(if board {"Show PCB pad"} else {"Show schematic pin"}).click();h.run_steps(5);
    assert_eq!(engine.session_state(),before);
    let mut app=h.into_state();
    let options=eframe::NativeOptions {
        viewport:egui::ViewportBuilder::default().with_title("Viamind terminal probe inspection").with_app_id("viamind-pin-pad-inspection").with_inner_size([width,1000.0]).with_min_inner_size([width,1000.0]).with_max_inner_size([width,1000.0]).with_resizable(false),
        renderer:eframe::Renderer::Wgpu,..Default::default()
    };
    eframe::run_native("Viamind terminal probe inspection",options,Box::new(move|cc|{
        app.configure_renderer(cc);
        Ok(Box::new(Inspect{app,engine,before,terminal}))
    })).map_err(|e|e.to_string())?;
    Ok(())
}
