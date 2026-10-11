//! Inspection-only production artwork editor; actual egui edits, native WGPU readback.
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_ui_egui::ViamindApp;
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    ui_passes: usize,
    controls: Vec<egui::Id>,
    field: Option<egui::Id>,
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
        let content = ui.ctx().content_rect();
        let native_buttons = if self.ui_passes >= 3 && !self.controls.is_empty() {
            let window = ui.ctx().memory(|m| m.area_rect(egui::Id::new("footprint-artwork-editor"))).expect("production artwork modal");
            assert!(content.contains_rect(window), "actual native modal fits: {window:?}; content {content:?}");
            self.controls
                .iter()
                .map(|id| {
                    let response = ui.ctx().read_response(*id).expect("exact actual native artwork footer button ID");
                    assert!(window.contains_rect(response.rect) && content.contains_rect(response.rect));
                    assert!(response.enabled());
                    response.rect
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let native_field = if self.ui_passes >= 3 {
            self.field.map(|id| {
                let response = ui.ctx().read_response(id).expect("actual native prepared silk width field");
                let window = ui.ctx().memory(|m| m.area_rect(egui::Id::new("footprint-artwork-editor"))).unwrap();
                assert!(content.contains_rect(response.rect) && window.contains_rect(response.rect));
                response.rect
            })
        } else {
            None
        };
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            // Requested logical size may map to a 2x native DPI framebuffer.
            // Keep readback bounded at 16M pixels (64 MiB original RGBA).
            assert!(image.pixels.len() <= 16_777_216, "bounded native readback {:?}", image.size);
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
                serde_json::json!({"capture":"eframe native WGPU Screenshot event","pid":std::process::id(),"path":path,"rgba_path":rgba_path,"alpha_min":alpha_min,"alpha_max":alpha_max,"nonopaque_alpha_pixels":nonopaque_alpha_pixels,"rgba_representation":"original egui Screenshot Color32 channels","rgb_review":"same RGB samples; alpha omitted for review, no opaque readback assertion","native_viewport_transparent":false,"pixels":image.size,"ui_passes":self.ui_passes,"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"control_bounds":"actual native Apply artwork/Reload artwork/Cancel read_response rectangles using exact safely captured labeled widget IDs","controls":native_buttons.iter().map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(),"prepared_silk_width_bounds":native_field.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
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
    let board = std::env::args().nth(5).as_deref() == Some("board");
    let e = EngineHandle::new(viamind_doc::file::load(path)?);
    let part = e.read(|p| p.board.parts.values().next().unwrap().clone());
    assert!(part.artwork.as_ref().is_some_and(|a| !a.silkscreen.is_empty()));
    let before = e.session_state();
    let status = e.document_status();
    let mut app = ViamindApp::with_design(e.clone(), None);
    if !board {
        app.begin_edit_footprint_artwork(part.clone());
    }
    let mut h = Harness::builder().with_size(vec2(width, height)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(6);
    let mut controls = Vec::new();
    if !board {
        input(&mut h, "Silk 1 Stroke width", "0.150003");
        for label in ["Apply artwork", "Reload artwork", "Cancel"] {
            h.get_by_label(label).focus();
            h.run_steps(3);
            assert!(h.get_by_label(label).is_focused());
            let id = h.ctx.memory(egui::Memory::focused).expect("labeled button focus yields safe egui ID");
            let response = h.ctx.read_response(id).expect("prepared footer button");
            assert_eq!(response.rect, h.get_by_label(label).rect());
            assert!(h.ctx.content_rect().contains_rect(response.rect));
            controls.push(id);
        }
    }
    let field = if board {
        None
    } else {
        // The field lives inside two scroll areas. Wheel over the outer courtyard
        // first; inner scroll_to_me alone does not establish ancestor visibility.
        h.hover_at(h.get_by_label("Courtyard min X").rect().center());
        h.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -480.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(12);
        input(&mut h, "Silk 1 Stroke width", "0.150003");
        let rect = h.get_by_label("Silk 1 Stroke width").rect();
        let clip = h
            .output()
            .shapes
            .iter()
            .filter_map(|shape| {
                let egui::Shape::Rect(background) = &shape.shape else { return None };
                (background.rect.contains(rect.center())
                    && (background.rect.width() - rect.width()).abs() <= 4.0
                    && (background.rect.height() - rect.height()).abs() <= 4.0)
                    .then_some(shape.clip_rect.intersect(h.ctx.content_rect()))
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("actual painted silk-width input");
        assert!(clip.contains_rect(rect), "prepared field {rect:?} within actual effective clip {clip:?}");
        Some(h.ctx.memory(egui::Memory::focused).expect("prepared silk width focus"))
    };
    let memory = h.ctx.memory(Clone::clone);
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    println!(
        "{}",
        serde_json::json!({"actual_egui_text_edit_preparation":!board,"local_fields_only_unchecked":!board,"target_id":part.id,"logical_focus_retained_only_in_inspection":true,"os_input":false,"mode":if board {"board"} else {"editor"}})
    );
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind footprint artwork inspection")
            .with_app_id("viamind-footprint-artwork-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind footprint artwork inspection",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.memory_mut(|m| *m = memory);
            app.configure_renderer(cc);
            Ok(Box::new(Inspect { app, engine: e, before, status, capture: Some(capture), ui_passes: 0, controls, field }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
