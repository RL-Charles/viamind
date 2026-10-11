//! Inspection-only host: actual local egui Query input and original native screenshot channels.
use std::{io::Write, path::PathBuf};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    native: Vec<u8>,
    path: PathBuf,
    output: PathBuf,
    target: Option<egui::Pos2>,
    pressed: bool,
    clicked: bool,
    capturing: bool,
    finished: bool,
    frames: usize,
    events: usize,
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.app, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
        self.frames += 1;
        assert!(self.frames < 1500, "bounded inspection must complete");
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status().path.as_ref(), Some(&self.path));
        let layers = ui.ctx().memory(|m| m.areas().visible_layer_ids());
        let painted: Vec<_> = ui.ctx().graphics(|g| {
            layers
                .iter()
                .flat_map(|layer| g.get(*layer).into_iter().flat_map(egui::layers::PaintList::all_entries))
                .filter_map(|entry| {
                    let egui::Shape::Text(text) = &entry.shape else { return None };
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    entry.clip_rect.contains_rect(rect).then(|| (text.galley.text().to_owned(), rect))
                })
                .collect()
        });
        if !self.clicked && !self.pressed {
            self.target = painted.iter().find(|(s, _)| s == "Query all nets").map(|(_, rect)| rect.center());
        }
        if let Some(report) = self.app.unrouted.report()
            && !self.capturing
            && !self.finished
        {
            assert!(self.clicked);
            assert_eq!(report.nets.len(), 7);
            assert_eq!((report.connected_count, report.unrouted_count, report.unknown_count), (2, 1, 1));
            assert_eq!(report.source_cursor, self.before.cursor);
            for expected in
                ["Query all nets", "Coverage: incomplete", "7 nets · 1 unrouted · 1 unknown · 2 connected · 3 with fewer than two terminals"]
            {
                assert!(painted.iter().any(|(label, _)| label == expected), "readable {expected}");
            }
            self.capturing = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image =
            ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if self.capturing
            && !self.finished
            && let Some(image) = image
        {
            assert_eq!(image.size, [1600, 1000]);
            assert_eq!(std::fs::read(&self.path).unwrap(), self.native);
            let rgba: Vec<_> = image.pixels.iter().flat_map(egui::Color32::to_array).collect();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            for (name, header, bytes) in [
                ("native.p7", b"P7\nWIDTH 1600\nHEIGHT 1000\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n".as_slice(), rgba.as_slice()),
                ("native.p6", b"P6\n1600 1000\n255\n".as_slice(), rgb.as_slice()),
            ] {
                let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join(name)).unwrap();
                f.write_all(header).unwrap();
                f.write_all(bytes).unwrap();
                f.sync_all().unwrap();
            }
            let report = serde_json::json!({"passed":true,"route":"production ViamindApp with actual local egui input and native WGPU Screenshot event", "pid":std::process::id(),"executable":std::env::current_exe().unwrap(),"frames":self.frames,"local_injected_events":self.events,"full_session_unchanged":true,"native_path_and_bytes_unchanged":true,"source_cursor":self.before.cursor,"result":self.app.unrouted.report().unwrap(),"visible_labels":painted.iter().map(|(s,_)|s).collect::<Vec<_>>(),"width":1600,"height":1000,"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"nonopaque_pixels":image.pixels.iter().filter(|p| p.a()!=255).count(),"channels":"original native Color32 RGBA plus unchanged RGB; no unpremultiplication or flattening", "claims_excluded":["OS input","physical continuity","manufacturing certification","frame cadence"]});
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join("native-report.json")).unwrap();
            serde_json::to_writer_pretty(&mut f, &report).unwrap();
            f.sync_all().unwrap();
            self.finished = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if !self.clicked
            && let Some(pos) = self.target
        {
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: !self.pressed,
                modifiers: egui::Modifiers::NONE,
            });
            self.events += 2;
            if self.pressed {
                self.clicked = true;
            }
            self.pressed = !self.pressed;
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("existing native project required")?);
    let output = PathBuf::from(args.next().ok_or("NEW capture directory required")?);
    std::fs::create_dir(&output)?;
    let engine = viamind_engine::EngineHandle::load(&path)?;
    let path = engine.document_status().path.ok_or("loaded native path required")?;
    let native = std::fs::read(&path)?;
    let before = engine.session_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind unrouted-net inspection")
            .with_app_id("viamind-unrouted-inspection")
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([1600., 1000.])
            .with_max_inner_size([1600., 1000.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind unrouted-net inspection",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_design(engine.clone(), None);
            app.configure_renderer(cc);
            app.begin_unrouted_nets();
            Ok(Box::new(Inspection {
                app,
                engine,
                before,
                native,
                path,
                output,
                target: None,
                pressed: false,
                clicked: false,
                capturing: false,
                finished: false,
                frames: 0,
                events: 0,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
