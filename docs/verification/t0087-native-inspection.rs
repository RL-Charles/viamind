use viamind_ui_egui::ViamindApp;
struct Inspection { inner:ViamindApp, frames:u32 }
impl eframe::App for Inspection {
 fn update(&mut self,ctx:&egui::Context,frame:&mut eframe::Frame) {
  eframe::App::update(&mut self.inner,ctx,frame);self.frames+=1;if self.frames<40 {ctx.request_repaint();}
 }
 fn raw_input_hook(&mut self,_ctx:&egui::Context,input:&mut egui::RawInput) {
  // One explicitly documented inspection-only egui scroll; production behavior unchanged.
  if self.frames==25 {
   input.events.push(egui::Event::PointerMoved(egui::pos2(130.0,800.0)));
   input.events.push(egui::Event::MouseWheel {unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-600.0),modifiers:egui::Modifiers::NONE});
  }
 }
}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let path=std::env::args_os().nth(1).ok_or("project required")?;
 let engine=viamind_engine::EngineHandle::load(&path)?;
 let first=engine.read(|p|p.board.parts.keys().next().copied()).ok_or("component required")?;
 engine.set_selection(viamind_engine::SetSelection{ids:vec![first],mode:Default::default()},viamind_engine::Origin::User)?;
 let port=std::env::args().nth(2).unwrap_or_else(||"7915".into());
 let server=viamind_mcp::spawn_engine_server(engine.clone(),format!("127.0.0.1:{port}").parse()?)?;
 let options=eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_title("Viamind sourcing inspection").with_app_id("viamind-sourcing-inspection").with_inner_size([1600.0,1000.0]).with_min_inner_size([1600.0,1000.0]).with_max_inner_size([1600.0,1000.0]).with_resizable(false),renderer:eframe::Renderer::Wgpu,..Default::default()};
 let url=server.url();eframe::run_native("Viamind sourcing inspection",options,Box::new(move|cc|{let mut app=ViamindApp::with_design(engine,Some(url));app.configure_renderer(cc);Ok(Box::new(Inspection{inner:app,frames:0}))})).map_err(|e|e.to_string())?;
 server.shutdown();Ok(())
}
