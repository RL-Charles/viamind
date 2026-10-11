//! Production ViamindApp historical release inspection with one local egui button click.
use std::{io::Write, path::PathBuf};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: viamind_engine::EngineHandle,
    before: viamind_engine::SessionSnapshot,
    target: Option<egui::Pos2>,
    resize_start: Option<egui::Pos2>,
    inputs: u32,
    clicks: u32,
    requested: bool,
    capture: Option<PathBuf>,
}
impl eframe::App for Inspection {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.app, ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
        assert_eq!(self.engine.session_state(), self.before);
        let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Inspect saved release")));
        let (target, visible) = ui.ctx().graphics(|graphics| {
            let mut target = None;
            let mut visible = Vec::new();
            if let Some(list) = graphics.get(layer) {
                for entry in list.all_entries() {
                    if let egui::Shape::Text(text) = &entry.shape {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        if entry.clip_rect.contains_rect(rect) {
                            visible.push(text.galley.text().to_owned());
                            if text.galley.text() == "Inspect release" {
                                target = Some(rect.center());
                            }
                        }
                    }
                }
            }
            (target, visible)
        });
        self.target = target;
        let calibrated = self.app.release_inspection.result.is_some();
        let controls = [
            "Saved release directory",
            "Inspect release",
            "Matched populated component IDs: 2",
            "Checks performed",
            "Checks not performed",
            "No authenticated file-content digest, original-export authenticity or current-board equality",
            "No vendor/manufacturing/stock/polarity acceptance",
        ]
        .iter()
        .all(|label| visible.iter().any(|text| text == label));
        if self.inputs == 200 {
            println!(
                "{}",
                serde_json::json!({"inspection_progress":self.inputs,"clicks":self.clicks,"target":format!("{:?}", self.target),"calibrated":calibrated,"controls":controls,"visible":visible})
            );
        }
        assert!(self.inputs <= 200, "bounded inspection did not complete");
        if let Some(path) = &self.capture {
            let screenshot = ui.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = screenshot {
                assert!(calibrated && controls && self.clicks == 2);
                assert_eq!(image.size, [1600, 1000]);
                assert!(image.pixels.iter().all(|pixel| pixel.a() == 255));
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).expect("NEW capture");
                write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).expect("PPM header");
                let rgb: Vec<_> = image.pixels.iter().flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()]).collect();
                file.write_all(&rgb).expect("RGB pixels");
                file.sync_all().expect("capture sync");
                println!(
                    "{}",
                    serde_json::json!({"capture":"eframe native WGPU Screenshot event","pixels":image.size,"path":path,"pid":std::process::id(),"executable":std::env::current_exe().unwrap(),"local_inspect_button_pointer_events":self.clicks,"local_resize_pointer_events":3,"historical_release_result_present":calibrated,"inspection_controls_result_and_limitations_visible":controls,"visible_inspection_labels":visible,"full_session_unchanged":true,"source_cursor":self.before.cursor,"requested_release":self.app.release_inspection.path,"result":self.app.release_inspection.result,"path_setup":"explicit harness argument; native button has2 local pointer events"})
                );
                std::io::stdout().flush().expect("report flush");
                self.capture = None;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.inputs >= 65 && calibrated && controls && !self.requested {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.requested = true;
            }
        }
        if self.capture.is_some() {
            ui.ctx().request_repaint();
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.inputs += 1;
        // The production scrollable egui Window starts at 420px high. Resize only this
        // inspection-local dialog from its current native geometry, preserving production defaults.
        if self.inputs == 50 {
            self.resize_start =
                ctx.memory(|m| m.area_rect(egui::Id::new(Some("Inspect saved release")))).map(|r| r.right_bottom() - egui::vec2(3., 3.));
        }
        if matches!(self.inputs, 50..=52)
            && let Some(start) = self.resize_start
        {
            let pos = if self.inputs == 50 { start } else { start + egui::vec2(0., 380.) };
            input.events.push(egui::Event::PointerMoved(pos));
            if self.inputs != 51 {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: self.inputs == 50,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }

        if matches!(self.inputs, 25 | 26)
            && let Some(pos) = self.target
        {
            self.clicks += 1;
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: self.inputs == 25,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project = args.next().ok_or("project required")?;
    let release = args.next().ok_or("release required")?.into_string().map_err(|_| "release path must be UTF-8")?;
    let capture = PathBuf::from(args.next().ok_or("NEW capture required")?);
    let engine = viamind_engine::EngineHandle::load(project)?;
    let before = engine.session_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind saved release consistency inspection")
            .with_app_id("viamind-release-consistency-inspection")
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([1600., 1000.])
            .with_max_inner_size([1600., 1000.])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind saved release consistency inspection",
        options,
        Box::new(move |cc| {
            let mut app = ViamindApp::with_design(engine.clone(), None);
            app.configure_renderer(cc);
            app.begin_release_inspection();
            app.release_inspection.path = release.clone();
            Ok(Box::new(Inspection {
                app,
                engine,
                before,
                target: None,
                resize_start: None,
                inputs: 0,
                clicks: 0,
                requested: false,
                capture: Some(capture),
            }))
        }),
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
