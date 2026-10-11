//! Actual 640×400 compact input prepares production UI; native WGPU readback self-closes.
use egui_kittest::kittest::{NodeT, Queryable};
#[path = "../tests/support/symbol_compact.rs"]
mod prepare;
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
    controls: Vec<egui::Rect>,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        if self.capture.is_none() {
            // Skip the production dirty-document close dialog once immutable proof is saved.
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        self.ui_passes += 1;
        let content = ui.ctx().content_rect();
        assert!(self.controls.iter().all(|r| content.contains_rect(*r)), "prepared controls must fit actual native viewport");
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304);
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let path = self.capture.take().expect("one new capture");
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).expect("NEW readback");
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let pixels: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&pixels).unwrap();
            file.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"eframe native WGPU Screenshot event", "pid":std::process::id(), "path":path, "pixels":image.size, "ui_passes":self.ui_passes, "full_session_unchanged":true, "document_status_unchanged":true, "self_close_after_capture":true, "controls":self.controls.iter().map(|r| [r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(), "content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 180, "native screenshot did not complete");
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "mode row|completed, NEW capture path, NEW detached file path");
    let mode = &args[1];
    assert!(matches!(mode.as_str(), "row" | "completed"));
    let capture = PathBuf::from(&args[2]);
    let output = PathBuf::from(&args[3]);
    assert!(!capture.exists() && !output.exists());
    let engine = engine();
    let before = engine.session_state();
    let status = engine.document_status();
    let mut h = harness(&engine);
    click(&mut h, "New detached definition");
    input(&mut h, "Definition name", "Original compact symbol");
    click(&mut h, "Add definition pin");
    for (label, value) in [("Pin 2 number", "VIN"), ("Pin 2 name", "Retained input"), ("Pin 2 X (mm)", "1.000003"), ("Pin 2 Y (mm)", "-2.000007")] {
        input(&mut h, label, value);
    }
    let fields = ["number", "name", "X (mm)", "Y (mm)", "electrical type"];
    let ids: Vec<_> = fields.iter().map(|field| h.get_by_label(&format!("Pin 2 {field}")).accesskit_node().id()).collect();
    click(&mut h, "Remove pin 1");
    for (i, field) in fields.iter().enumerate() {
        assert_eq!(h.get_by_label(&format!("Pin 1 {field}")).accesskit_node().id(), ids[i]);
    }
    click(&mut h, "Pin 1 electrical type");
    click(&mut h, "Input");
    let controls = if mode == "row" {
        reveal(&mut h, "Remove pin 1");
        for _ in 0..3 {
            let button = h.get_by_label("Remove pin 1").rect();
            let union = fields.iter().map(|field| h.get_by_label(&format!("Pin 1 {field}")).rect()).fold(button, egui::Rect::union);
            let clip = painted_clip(&h, button);
            if clip.contains_rect(union) {
                break;
            }
            wheel(&mut h, button.center(), clip.center().y - union.center().y);
        }
        let button = h.get_by_label("Remove pin 1").rect();
        let mut controls: Vec<_> = fields.iter().map(|field| h.get_by_label(&format!("Pin 1 {field}")).rect()).collect();
        controls.push(button);
        let clip = painted_clip(&h, button);
        assert!(controls.iter().all(|r| clip.contains_rect(*r)));
        println!(
            "{}",
            serde_json::json!({"complete_row_in_actual_clip":true,"inner_clip":[clip.min.x,clip.min.y,clip.max.x,clip.max.y],"stable_pin_ids":true})
        );
        controls
    } else {
        input(&mut h, "NEW definition file path", &output.to_string_lossy());
        click(&mut h, "Save NEW definition file");
        wait(&mut h, "Created detached definition");
        let saved = viamind_doc::symbol_library::load(&output)?;
        assert_eq!(saved.pins.len(), 1);
        assert_eq!(saved.pins[0].number, "VIN");
        assert_eq!(saved.pins[0].position.x.0, 1_000_003);
        assert_eq!(saved.pins[0].position.y.0, -2_000_007);
        assert_eq!(saved.pins[0].electrical, viamind_doc::schematic::PinType::Input);
        h.get_by_label_contains("Created detached definition").scroll_to_me();
        h.run_steps(8);
        vec![h.get_by_label_contains("Created detached definition").rect()]
    };
    assert!(controls.iter().all(|r| h.ctx.content_rect().contains_rect(*r)));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind compact symbol inspection")
            .with_app_id("viamind-compact-symbol-inspection")
            .with_inner_size([640.0, 400.0])
            .with_min_inner_size([640.0, 400.0])
            .with_max_inner_size([640.0, 400.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind compact symbol inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture: Some(capture), ui_passes: 0, controls }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
