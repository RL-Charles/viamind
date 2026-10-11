use viamind_ui_egui::ViamindApp;
fn main()->Result<(),Box<dyn std::error::Error>>{
 let path=std::env::args_os().nth(1).ok_or("project required")?;
 let engine=viamind_engine::EngineHandle::load(&path)?;
 let port=std::env::args().nth(2).unwrap_or_else(||"7905".into());
 let server=viamind_mcp::spawn_engine_server(engine.clone(),format!("127.0.0.1:{port}").parse()?)?;
 let options=eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_title("Viamind assembly CSV inspection").with_app_id("viamind-assembly-csv-inspection").with_inner_size([1600.0,1000.0]).with_min_inner_size([1600.0,1000.0]).with_max_inner_size([1600.0,1000.0]).with_resizable(false),renderer:eframe::Renderer::Wgpu,..Default::default()};
 let url=server.url();eframe::run_native("Viamind assembly CSV inspection",options,Box::new(move|cc|{let mut app=ViamindApp::with_design(engine,Some(url));app.configure_renderer(cc);app.begin_assembly_export();Ok(Box::new(app))})).map_err(|e|e.to_string())?;
 server.shutdown();Ok(())
}
