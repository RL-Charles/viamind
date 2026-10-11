//! Inspection-only production package annotation review; actual egui edits, native WGPU readback.
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
        let window = ui.ctx().memory(|m| m.area_rect(egui::Id::new("schematic-annotation-dialog"))).expect("production annotation window");
        let content = ui.ctx().content_rect();
        // Resolve the exact labeled buttons' safe egui IDs in the native pass.
        // Prepared absolute coordinates and font/pixel-grid dimensions can differ
        // from the negotiated native viewport and are not native bounds evidence.
        let native_buttons = if self.ui_passes >= 3 {
            assert!(content.contains_rect(window), "actual native annotation window fits: {window:?}; content {content:?}");
            self.controls
                .iter()
                .map(|id| {
                    let response = ui.ctx().read_response(*id).expect("same actual native footer button ID");
                    assert!(window.contains_rect(response.rect) && content.contains_rect(response.rect));
                    assert!(response.enabled());
                    response.rect
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
                serde_json::json!({"capture":"eframe native WGPU Screenshot event","pid":std::process::id(),"path":path,"rgba_path":rgba_path,"alpha_min":alpha_min,"alpha_max":alpha_max,"nonopaque_alpha_pixels":nonopaque_alpha_pixels,"rgba_representation":"original egui Screenshot Color32 channels","rgb_review":"same RGB samples; alpha omitted for review, no opaque readback assertion","native_viewport_transparent":false,"pixels":image.size,"ui_passes":self.ui_passes,"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"control_bounds":"actual native Prepare/Apply/Cancel read_response rectangles using exact safely captured labeled widget IDs","controls":native_buttons.iter().map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(),"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
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
    let e = EngineHandle::new(viamind_doc::file::load(path)?);
    let before = e.session_state();
    let status = e.document_status();
    let app = ViamindApp::with_design(e.clone(), None);
    let mut h = Harness::builder().with_size(vec2(1600.0, 1800.0)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    h.run_steps(6);
    h.get_by_label("Annotate symbols").click();
    h.run_steps(5);
    h.set_size(vec2(width, height));
    h.run_steps(8);
    h.get_by_label("Renumber all symbols").click();
    h.run_steps(4);
    input(&mut h, "Annotation starting number", "5");
    h.get_by_label("Prepare annotation").click();
    h.run_steps(6);
    assert!(h.query_by_label("12 symbols/units, 2 packages, 12 proposed member reference changes").is_some());
    assert!(h.query_by_label_contains("U? to U5 - unit A").is_some());
    let mut controls = Vec::new();
    for label in ["Prepare annotation", "Apply annotation", "Cancel annotation preview"] {
        h.get_by_label(label).focus();
        h.run_steps(3);
        assert!(h.get_by_label(label).is_focused());
        let id = h.ctx.memory(egui::Memory::focused).expect("labeled button focus gives its safe egui ID");
        let response = h.ctx.read_response(id).expect("prepared footer button");
        assert_eq!(response.rect, h.get_by_label(label).rect());
        assert!(response.rect.min.x >= 0. && response.rect.min.y >= 0. && response.rect.max.x <= width && response.rect.max.y <= height);
        controls.push(id);
    }
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    println!(
        "{}",
        serde_json::json!({"actual_egui_preparation":true,"checked_shared_annotation_preview":true,"all_package_refs_uncommitted":true,"symbols":12,"packages":2,"proposed_changes":12,"logical_focus_retained_only_in_inspection":true,"os_input":false})
    );
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind package annotation inspection")
            .with_app_id("viamind-package-annotation-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind package annotation inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            Ok(Box::new(Inspect { app, engine: e, before, status, capture: Some(capture), ui_passes: 0, controls }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
