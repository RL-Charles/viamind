//! Inspection-only production property editor; actual egui edits, native WGPU readback.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, Origin, SessionSnapshot, SetSelection};
use viamind_ui_egui::ViamindApp;
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    ui_passes: usize,
    controls: Vec<egui::Rect>,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        if self.capture.is_none() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        self.ui_passes += 1;
        let window = ui
            .ctx()
            .memory(|m| m.area_rect(egui::Id::new(Some(std::borrow::Cow::Borrowed("Copper properties")))))
            .expect("production property window");
        let content = ui.ctx().content_rect();
        // X11 can negotiate a different native viewport than the preparation
        // harness. Read actual completed-pass hit rectangles instead of treating
        // the harness's absolute control coordinates as native coordinates.
        let native_buttons = if self.ui_passes >= 3 {
            assert!(content.contains_rect(window), "actual native property window fits: {window:?}; content {content:?}");
            let rects = ui.ctx().interactive_rects_last_pass();
            self.controls[1..]
                .iter()
                .map(|prepared| {
                    let matches: Vec<_> = rects
                        .iter()
                        .copied()
                        .filter(|r| {
                            window.contains_rect(*r) && (r.width() - prepared.width()).abs() < 0.1 && (r.height() - prepared.height()).abs() < 0.1
                        })
                        .collect();
                    assert_eq!(matches.len(), 1, "unique native footer button size {:?}; window {window:?}; content {content:?}", prepared.size());
                    assert!(content.contains_rect(matches[0]));
                    matches[0]
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304);
            // X11 opaque surfaces can return nonopaque stored alpha; preserve every
            // original Screenshot-event channel separately from the RGB review view.
            let alpha_min = image.pixels.iter().map(egui::Color32::a).min().unwrap();
            let alpha_max = image.pixels.iter().map(egui::Color32::a).max().unwrap();
            let nonopaque_alpha_pixels = image.pixels.iter().filter(|p| p.a() != 255).count();
            let path = self.capture.take().unwrap();
            let rgba_path = path.with_extension("p7");
            let mut raw = std::fs::OpenOptions::new().write(true).create_new(true).open(&rgba_path).unwrap();
            write!(raw, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1]).unwrap();
            let rgba: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b(), p.a()]).collect();
            raw.write_all(&rgba).unwrap();
            raw.sync_all().unwrap();
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            write!(f, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let rgb: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            f.write_all(&rgb).unwrap();
            f.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"eframe native WGPU Screenshot event","pid":std::process::id(),"path":path,"rgba_path":rgba_path,"alpha_min":alpha_min,"alpha_max":alpha_max,"nonopaque_alpha_pixels":nonopaque_alpha_pixels,"rgba_representation":"original egui Screenshot Color32 channels","rgb_review":"same RGB samples; alpha omitted for review, no opaque readback assertion","native_viewport_transparent":false,"pixels":image.size,"ui_passes":self.ui_passes,"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"control_bounds":"three actual native completed-pass hit rectangles, each uniquely matched to prepared Apply/Reload/Cancel button dimensions","controls":native_buttons.iter().map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(),"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 180);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.retain(|e| !matches!(e, egui::Event::WindowFocused(false)));
    }
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(3);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(4);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("fixture")?;
    let capture = std::env::args_os().nth(2).map(PathBuf::from).ok_or("NEW capture")?;
    let width = std::env::args().nth(3).map_or(Ok(1600.), |s| s.parse::<f32>())?;
    let height = std::env::args().nth(4).map_or(Ok(1000.), |s| s.parse::<f32>())?;
    let via = std::env::args().nth(5).as_deref() == Some("via");
    let e = EngineHandle::new(viamind_doc::file::load(path)?);
    let id = e.read(|p| if via { *p.board.vias.keys().next().unwrap() } else { *p.board.tracks.keys().next().unwrap() });
    e.set_selection(SetSelection { ids: vec![id], mode: Default::default() }, Origin::User)?;
    let before = e.session_state();
    let status = e.document_status();
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_copper_properties();
    let mut h = Harness::builder().with_size(vec2(width, height)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(6);
    if via {
        input(&mut h, "Property via diameter (mm)", "0.900003");
        input(&mut h, "Property via drill (mm)", "0.400001");
    } else {
        input(&mut h, "Property track width (mm)", "0.350001");
        h.get_by_label("Property bottom copper").click();
        h.run_steps(4);
    }
    input(&mut h, "Property net (empty = unassigned)", "N");
    h.run_steps(6);
    let controls = ["Apply runs scoped checks; excluded objects are not certified.", "Apply properties", "Reload properties", "Cancel properties"]
        .into_iter()
        .map(|l| h.get_by_label(l).rect())
        .collect::<Vec<_>>();
    assert!(controls.iter().all(|r| r.min.x >= 0. && r.min.y >= 0. && r.max.x <= width && r.max.y <= height));
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    println!(
        "{}",
        serde_json::json!({"actual_egui_text_edit_preparation":true,"local_fields_only_unchecked":true,"target_kind":if via{"via"}else{"track"},"target_id":id,"logical_focus_retained_only_in_inspection":true,"os_input":false})
    );
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind copper properties inspection")
            .with_app_id("viamind-copper-properties-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind copper properties inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspect { app, engine: e, before, status, capture: Some(capture), ui_passes: 0, controls }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
