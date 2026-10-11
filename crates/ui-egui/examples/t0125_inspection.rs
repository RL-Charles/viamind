//! Inspection host: production UI with local pointer events, bounded read-only preview and original native RGBA capture.
use std::{io::Write, path::PathBuf};
use viamind_ui_egui::{ViamindApp, camera::WorldPos};
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    status: viamind_engine::DocumentStatus,
    native: Vec<u8>,
    path: PathBuf,
    output: PathBuf,
    phase: usize,
    frames: usize,
    last: usize,
    events: usize,
    wheel_events: usize,
    queued: Vec<egui::Event>,
    capturing: bool,
    finished: bool,
    size: [usize; 2],
    seen_controls: std::collections::BTreeSet<String>,
    window_wheel_events: usize,
}
impl Inspection {
    fn point(&self, x: f64, y: f64) -> egui::Pos2 {
        self.app.schematic.camera.world_to_screen(WorldPos::new(x, y), self.app.schematic.viewport)
    }
    fn click(&mut self, pos: egui::Pos2, pressed: bool) {
        self.queued.extend([
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
        ]);
    }
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.app, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.finished {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        eframe::App::ui(&mut self.app, ui, frame);
        self.frames += 1;
        assert!(self.frames < 1500, "bounded connected drag inspection");
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        let layers = ui.ctx().memory(|m| m.areas().visible_layer_ids());
        let painted: Vec<_> = ui.ctx().graphics(|g| {
            layers
                .iter()
                .flat_map(|l| g.get(*l).into_iter().flat_map(egui::layers::PaintList::all_entries))
                .filter_map(|entry| {
                    let egui::Shape::Text(text) = &entry.shape else { return None };
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    entry.clip_rect.intersect(ui.ctx().content_rect()).contains_rect(rect).then(|| (text.galley.text().to_owned(), rect))
                })
                .collect()
        });
        const REQUIRED: [&str; 6] =
            ["Delta X mm", "Delta Y mm", "Grid mm", "Preview connected move", "Apply connected move", "Cancel connected move"];
        for (label, _) in &painted {
            if REQUIRED.contains(&label.as_str()) {
                self.seen_controls.insert(label.clone());
            }
        }
        if self.frames - self.last >= 3 {
            match self.phase {
                0 if self.frames > 8 => {
                    self.click(self.point(20.0, 20.0), true);
                    self.phase = 1;
                    self.last = self.frames;
                }
                1 => {
                    self.click(self.point(20.0, 20.0), false);
                    self.phase = 2;
                    self.last = self.frames;
                }
                2 => {
                    if let Some((_, rect)) = painted.iter().find(|(s, _)| s == "Connected move selected symbol") {
                        self.click(rect.center(), true);
                        self.phase = 3;
                        self.last = self.frames;
                    } else if self.frames - self.last >= 8 {
                        assert!(self.wheel_events < 40, "connected move toolbar must be reachable through sidebar scroll");
                        self.queued.extend([
                            egui::Event::PointerMoved(egui::pos2(160.0, self.size[1] as f32 - 80.0)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -240.0),
                                phase: egui::TouchPhase::Move,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ]);
                        self.wheel_events += 1;
                        self.last = self.frames;
                    }
                }
                3 => {
                    let rect = painted.iter().find(|(s, _)| s == "Connected move selected symbol").expect("fresh painted toolbar while pressed").1;
                    self.click(rect.center(), false);
                    self.phase = 4;
                    self.last = self.frames;
                }
                4 if self.app.schematic_drag.active() => {
                    self.app.schematic_drag.open = false;
                    self.phase = 5;
                    self.last = self.frames;
                }
                5 => {
                    self.click(self.point(20.0, 20.0), true);
                    self.phase = 6;
                    self.last = self.frames;
                }
                6 => {
                    self.queued.push(egui::Event::PointerMoved(self.point(23.2, 25.3)));
                    self.phase = 7;
                    self.last = self.frames;
                }
                7 => {
                    self.click(self.point(23.2, 25.3), false);
                    self.phase = 8;
                    self.last = self.frames;
                }
                8 if self.app.schematic_drag.report().is_some() => {
                    let report = self.app.schematic_drag.report().unwrap();
                    assert_eq!((report.target_position.x.0, report.target_position.y.0), (23_000_000, 25_000_000));
                    assert_eq!(report.added_wire_count, 3);
                    assert!(report.native_graph_partition_preserved);
                    self.app.schematic_drag.open = true;
                    self.phase = 9;
                    self.last = self.frames;
                }
                9 if !self.capturing => {
                    let ready = REQUIRED.iter().all(|label| self.seen_controls.contains(*label))
                        && REQUIRED[3..].iter().all(|label| painted.iter().any(|(s, _)| s == label))
                        && painted.iter().any(|(s, _)| s.starts_with("Native graph preserved"));
                    if ready {
                        self.capturing = true;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                    } else {
                        assert!(self.window_wheel_events < 40, "fields and completion controls must be reachable through compact window scrolling");
                        let window =
                            ui.ctx().memory(|m| m.area_rect(egui::Id::new("connected_schematic_move"))).expect("fresh connected-move window");
                        let point = window.intersect(ui.ctx().content_rect()).center();
                        self.queued.extend([
                            egui::Event::PointerMoved(point),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -80.0),
                                phase: egui::TouchPhase::Move,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ]);
                        self.window_wheel_events += 1;
                        self.last = self.frames;
                    }
                }
                _ => {}
            }
        }
        let image =
            ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if self.capturing
            && let Some(image) = image
        {
            assert_eq!(image.size, self.size);
            assert_eq!(std::fs::read(&self.path).unwrap(), self.native);
            assert_eq!(self.engine.document_status(), self.status);
            let rgba: Vec<_> = image.pixels.iter().flat_map(egui::Color32::to_array).collect();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            let pam_header = format!("P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", self.size[0], self.size[1]);
            let ppm_header = format!("P6\n{} {}\n255\n", self.size[0], self.size[1]);
            for (name, header, bytes) in [("native.p7", pam_header.as_bytes(), rgba.as_slice()), ("native.p6", ppm_header.as_bytes(), rgb.as_slice())]
            {
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join(name)).unwrap();
                file.write_all(header).unwrap();
                file.write_all(bytes).unwrap();
                file.sync_all().unwrap();
            }
            let report = serde_json::json!({"passed":true,"route":"production ViamindApp pointer selection, actual visible toolbar click, local drag and native WGPU Screenshot event","pid":std::process::id(),"executable":std::env::current_exe().unwrap(),"frames":self.frames,"local_injected_events":self.events,"sidebar_wheel_events":self.wheel_events,"window_wheel_events":self.window_wheel_events,"full_session_unchanged":true,"complete_document_status_unchanged":true,"document_status":self.status,"native_path_and_bytes_unchanged":true,"source_cursor":self.before.cursor,"preview":self.app.schematic_drag.report(),"visible_labels":painted.iter().map(|(s,_)|s).collect::<Vec<_>>(),"effective_clip_seen_controls":self.seen_controls,"width":self.size[0],"height":self.size[1],"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"nonopaque_pixels":image.pixels.iter().filter(|p|p.a()!=255).count(),"channels":"original Color32 RGBA and identical RGB; no unpremultiplication or flattening","host_only_preparation":"The host hides/reopens the connected-move window around canvas dragging; viewport is fixed. Fields are verified as reachable across actual scrolling, with all completion buttons and result visible at capture. No model changes or OS input.","claims_excluded":["OS input","ERC/DRC certification","frame cadence"]});
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join("native-report.json")).unwrap();
            serde_json::to_writer_pretty(&mut file, &report).unwrap();
            file.sync_all().unwrap();
            self.finished = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        self.events += self.queued.len();
        input.events.append(&mut self.queued);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("existing native required")?);
    let output = PathBuf::from(args.next().ok_or("NEW output directory required")?);
    let size = match args.next() {
        None => [1600, 1000],
        Some(flag) if flag == "--compact" => [640, 400],
        _ => return Err("only native, NEW output directory and optional --compact accepted".into()),
    };
    if args.next().is_some() {
        return Err("unexpected inspection argument".into());
    }
    std::fs::create_dir(&output)?;
    let engine = viamind_engine::EngineHandle::load(&path)?;
    let path = engine.document_status().path.ok_or("native path")?;
    let native = std::fs::read(&path)?;
    let before = engine.session_state();
    let status = engine.document_status();
    let viewport_size = [size[0] as f32, size[1] as f32];
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind connected symbol move inspection")
            .with_app_id("viamind-connected-move-inspection")
            .with_inner_size(viewport_size)
            .with_min_inner_size(viewport_size)
            .with_max_inner_size(viewport_size)
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind connected symbol move inspection",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_design(engine.clone(), None);
            app.configure_renderer(cc);
            app.set_schematic_mode(true);
            Ok(Box::new(Inspection {
                app,
                engine,
                before,
                status,
                native,
                path,
                output,
                phase: 0,
                frames: 0,
                last: 0,
                events: 0,
                wheel_events: 0,
                queued: vec![],
                capturing: false,
                finished: false,
                size,
                seen_controls: Default::default(),
                window_wheel_events: 0,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
