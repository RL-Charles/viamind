//! Inspection-only host: actual local egui inputs into production complete-package UI.
use std::{io::Write, path::PathBuf};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    native_path: Option<PathBuf>,
    footprint: String,
    output: PathBuf,
    phase: usize,
    target: Option<egui::Pos2>,
    pressed: bool,
    resize_start: Option<egui::Pos2>,
    events: usize,
    frames: usize,
    capture: Option<&'static str>,
    preview_done: bool,
    created_done: bool,
    visible: Vec<String>,
}
impl Inspection {
    fn capture(&mut self, image: &egui::ColorImage, name: &str) {
        assert_eq!(image.size, [1600, 1000]);
        assert!(image.pixels.iter().all(|p| p.a() == 255));
        let path = self.output.join(name);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).expect("NEW P6");
        file.write_all(b"P6\n1600 1000\n255\n").unwrap();
        let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
        file.write_all(&rgb).unwrap();
        file.sync_all().unwrap();
    }
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.app, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
        self.frames += 1;
        assert!(self.frames < 1500, "inspection bounded; phase{} labels{:?}", self.phase, self.visible);
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
        self.visible = painted.iter().map(|(s, _)| s.clone()).collect();
        let named = |label: &str| painted.iter().find(|(s, _)| s == label).map(|(_, rect)| rect.center());
        let session = self.engine.session_state();
        if self.phase < 7 {
            assert_eq!(session, self.before, "preview cannot mutate session");
        }
        if !self.pressed {
            self.target = match self.phase {
                0 => named("Choose complete package"),
                1 => named("U1 · 5 units · Authored five-unit package"),
                2 => ui.ctx().read_response(egui::Id::new("package_component_path")).map(|r| r.rect.center()),
                3 => named("Package bottom"),
                4 => named("Package 90°"),
                5 => named("Prepare complete package component"),
                7 => named("Create one package PCB component"),
                8 => {
                    if session.project.board.parts.len() == 1 {
                        named("PCB")
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
        let ready_preview = self.visible.iter().any(|s| s == "One U1 · 5 units · 14 pads")
            && self.visible.iter().any(|s| s == "Origin (40000000, 40000000) nm · Bottom · 90°");
        if self.phase == 6 && ready_preview && self.capture.is_none() && !self.preview_done {
            assert!(self.visible.iter().any(|s| s == "Origin (40000000, 40000000) nm · Bottom · 90°"));
            self.capture = Some("native-preview.ppm");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if self.phase == 9 && !self.created_done && self.capture.is_none() {
            assert_eq!(session.project.schematic, self.before.project.schematic);
            assert_eq!(session.selection, self.before.selection);
            assert_eq!(session.cursor.stream_id, self.before.cursor.stream_id);
            assert_eq!(session.cursor.sequence, self.before.cursor.sequence + 1);
            assert_eq!(session.project.board.parts.len(), 1);
            let part = session.project.board.parts.values().next().unwrap();
            assert!(part.schematic_package.is_some());
            assert_eq!(part.pads.as_ref().unwrap().0.len(), 14);
            assert!(self.visible.iter().any(|s| s == "Created one linked physical component. Undo removes it; run DRC to inspect placement."));
            self.capture = Some("native-created.ppm");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let screenshot =
            ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let (Some(name), Some(image)) = (self.capture, &screenshot) {
            self.capture(image, name);
            self.capture = None;
            if !self.preview_done {
                self.preview_done = true;
                self.phase = 7;
            } else {
                self.created_done = true;
                let report = serde_json::json!({"passed":true,"route":"production ViamindApp with local egui events and native WGPU Screenshot","preview_full_session_unchanged":true,"created_one_complete_owner":true,"all_schematic_ids_and_geometry_unchanged":true,"shared_selection_unchanged":true,"one_design_transaction":true,"native_path_unchanged":self.engine.document_status().path==self.native_path,"before_cursor":self.before.cursor,"after_cursor":session.cursor,"local_injected_events":self.events,"local_resize_frames":3,"local_resize_pointer_events":5,"frames":self.frames,"pid":std::process::id(),"executable":std::env::current_exe().unwrap(),"footprint":self.footprint,"preview_capture":"native-preview.ppm","created_capture":"native-created.ppm","visible_final_labels":self.visible,"claims_excluded":["OS input","frame cadence","vendor acceptance","physical copper continuity"]});
                let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(self.output.join("native-report.json")).unwrap();
                serde_json::to_writer_pretty(&mut f, &report).unwrap();
                f.sync_all().unwrap();
                println!("{report}");
                std::io::stdout().flush().unwrap();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.frames == 20 {
            self.resize_start = ctx
                .memory(|m| m.area_rect(egui::Id::new(Some("Complete package to one PCB component"))))
                .map(|r| r.right_bottom() - egui::vec2(3., 3.));
        }
        if (20..=22).contains(&self.frames)
            && let Some(start) = self.resize_start
        {
            let pos = if self.frames == 20 { start } else { start + egui::vec2(0., 400.) };
            input.events.push(egui::Event::PointerMoved(pos));
            if self.frames != 21 {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: self.frames == 20,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        if self.frames < 30 {
            return;
        }
        if self.phase == 7 && self.target.is_none() && self.frames.is_multiple_of(10) {
            if let Some(rect) = ctx.memory(|m| m.area_rect(egui::Id::new(Some("Complete package to one PCB component")))) {
                input.events.push(egui::Event::PointerMoved(rect.left_top() + egui::vec2(25., 100.)));
                input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -200.),
                    modifiers: egui::Modifiers::NONE,
                });
                self.events += 2;
            }
        }
        if let Some(pos) = self.target {
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: !self.pressed,
                modifiers: egui::Modifiers::NONE,
            });
            self.events += 2;
            if self.pressed {
                if self.phase == 2 {
                    input.events.push(egui::Event::Text(self.footprint.clone()));
                    self.events += 1;
                }
                self.phase += 1;
            }
            self.pressed = !self.pressed;
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project = args.next().ok_or("placed package native required")?;
    let footprint = args.next().ok_or("footprint path required")?.into_string().map_err(|_| "UTF8 path required")?;
    let output = PathBuf::from(args.next().ok_or("NEW capture directory required")?);
    std::fs::create_dir(&output)?;
    let engine = viamind_engine::EngineHandle::load(project)?;
    let before = engine.session_state();
    let native_path = engine.document_status().path;
    assert!(before.project.board.parts.is_empty());
    assert_eq!(before.project.schematic.sheets.values().flat_map(|s| s.content.iter().map(|c| c.packages.len())).sum::<usize>(), 1);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind complete package PCB inspection")
            .with_app_id("viamind-package-pcb-inspection")
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([1600., 1000.])
            .with_max_inner_size([1600., 1000.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind complete package PCB inspection",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_design(engine.clone(), None);
            app.configure_renderer(cc);
            app.begin_package_component();
            Ok(Box::new(Inspection {
                app,
                engine,
                before,
                native_path,
                footprint,
                output,
                phase: 0,
                target: None,
                pressed: false,
                resize_start: None,
                events: 0,
                frames: 0,
                capture: None,
                preview_done: false,
                created_done: false,
                visible: Vec::new(),
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
