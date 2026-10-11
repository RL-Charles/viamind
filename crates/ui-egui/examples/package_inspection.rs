//! Inspection-only original package workflow; actual egui input and self-closing native WGPU readback.
#[allow(dead_code, reason = "Shared input harness also tests file loading outside this authoring-only native host")]
#[path = "../tests/support/package_compact.rs"]
mod prepare;
use egui_kittest::{Harness, kittest::Queryable};
use prepare::*;
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
    placed: bool,
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
        let native_buttons = if self.ui_passes >= 3 {
            if self.placed {
                assert!(content.contains_rect(self.app.schematic.viewport));
            }
            self.controls
                .iter()
                .map(|id| {
                    let response = ui.ctx().read_response(*id).expect("same native labeled widget identity");
                    assert!(content.contains_rect(response.rect) && response.enabled(), "actual native package control {response:?}");
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
                serde_json::json!({"capture":"eframe native WGPU Screenshot event","pid":std::process::id(),"path":path,"rgba_path":rgba_path,"alpha_min":alpha_min,"alpha_max":alpha_max,"nonopaque_alpha_pixels":nonopaque_alpha_pixels,"rgba_representation":"original egui Screenshot Color32 channels","rgb_review":"same RGB samples; alpha omitted for review, no opaque readback assertion","native_viewport_transparent":false,"pixels":image.size,"ui_passes":self.ui_passes,"full_session_unchanged":true,"document_status_unchanged":true,"self_close_after_capture":true,"control_bounds":"actual native package read_response rectangles using exact safely captured labeled widget IDs","controls":native_buttons.iter().map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(),"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "mode unit|completed|placed,NEW readback,NEW detached output");
    let mode = &args[1];
    assert!(matches!(mode.as_str(), "unit" | "completed" | "placed"));
    let capture = PathBuf::from(&args[2]);
    let output = PathBuf::from(&args[3]);
    assert!(!capture.exists() && !capture.with_extension("p7").exists() && !output.exists());
    let engine = engine();
    let initial = engine.session_state();
    let initial_status = engine.document_status();
    let mut h = harness(&engine);
    authored(&mut h);
    input(&mut h, "Pin 2 X (mm)", "1.000003");
    input(&mut h, "Pin 2 Y (mm)", "-2.000007");
    let mut controls = Vec::new();
    let placed = mode == "placed";
    if mode == "unit" {
        reveal(&mut h, "Remove pin 2");
        let labels = ["Pin 2 number", "Pin 2 name", "Pin 2 X (mm)", "Pin 2 Y (mm)", "Pin 2 electrical type", "Remove pin 2"];
        for _ in 0..3 {
            let button = h.get_by_label("Remove pin 2").rect();
            let union = labels.iter().map(|label| h.get_by_label(label).rect()).fold(button, egui::Rect::union);
            let clip = painted_clip(&h, button);
            if clip.contains_rect(union) {
                break;
            }
            wheel(&mut h, button.center(), clip.center().y - union.center().y);
        }
        let clip = painted_clip(&h, h.get_by_label("Remove pin 2").rect());
        for label in labels {
            assert!(clip.contains_rect(h.get_by_label(label).rect()));
            h.get_by_label(label).focus();
            h.run_steps(2);
            let id = h.ctx.memory(egui::Memory::focused).expect("labeled field/button focus");
            assert_eq!(h.ctx.read_response(id).unwrap().rect, h.get_by_label(label).rect());
            controls.push(id);
        }
        println!(
            "{}",
            serde_json::json!({"actual_compact_ancestor_clip":true,"clip":[clip.min.x,clip.min.y,clip.max.x,clip.max.y],"os_input_claim":false})
        );
    } else if mode == "completed" {
        input(&mut h, "Package file path (load or NEW save)", &output.to_string_lossy());
        click(&mut h, "Save NEW complete package file");
        wait(&mut h, "Created ");
        let saved = viamind_doc::symbol_package::load(&output)?;
        assert_eq!(saved.units.len(), 5);
        assert_eq!(saved.units[4].pins[1].position.x.0, 1_000_003);
        assert_eq!(saved.units[4].pins[1].position.y.0, -2_000_007);
        h.get_by_label_contains("Created ").scroll_to_me();
        h.run_steps(8);
    } else {
        click(&mut h, "Place complete package");
        wait(&mut h, "Complete package command applied");
        h.state_mut().package_library.close();
        let app = h.into_state();
        h = Harness::builder().with_size(egui::vec2(1600., 1100.)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
        h.run_steps(8);
        let report = engine.query_symbol_packages(&Default::default())?;
        assert_eq!(report.total, 1);
        assert_eq!(report.packages[0].units.len(), 5);
    }
    if !placed {
        assert_eq!(engine.session_state(), initial);
        assert_eq!(engine.document_status(), initial_status);
    }
    let before = engine.session_state();
    let status = engine.document_status();
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let size = if placed { [1600., 1100.] } else { [640., 400.] };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind explicit package inspection")
            .with_app_id("viamind-package-inspection")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind explicit package inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture: Some(capture), ui_passes: 0, controls, placed }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
