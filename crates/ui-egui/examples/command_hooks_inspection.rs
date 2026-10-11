//! Exact production UI configuration/veto/recovery with native WGPU pixels.
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use viamind_doc::Project;
use viamind_engine::{DocumentStatus, EngineHandle, Origin, SessionSnapshot};
use viamind_ui_egui::ViamindApp;
struct Inspection {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    native: PathBuf,
    bytes: Vec<u8>,
    capture: Option<PathBuf>,
    passes: usize,
}
impl eframe::App for Inspection {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        assert_eq!(std::fs::read(&self.native).unwrap(), self.bytes);
        if self.capture.is_none() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.));
        self.passes += 1;
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        let image =
            ui.ctx().input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(image) = image {
            assert!(image.pixels.len() <= 16_777_216);
            let content = ui.ctx().content_rect();
            let controls = self.app.command_hooks.control_rects();
            assert_eq!(controls.len(), 2);
            assert!(controls.iter().all(|(r, c)| content.contains_rect(*r) && c.contains_rect(*r)));
            let p = self.capture.take().unwrap();
            let rgba = p.with_extension("p7");
            let mut w = std::fs::OpenOptions::new().write(true).create_new(true).open(&rgba).unwrap();
            write!(w, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1]).unwrap();
            for pixel in &image.pixels {
                w.write_all(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]).unwrap();
            }
            w.sync_all().unwrap();
            let mut w = std::fs::OpenOptions::new().write(true).create_new(true).open(&p).unwrap();
            write!(w, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            for pixel in &image.pixels {
                w.write_all(&[pixel.r(), pixel.g(), pixel.b()]).unwrap();
            }
            w.sync_all().unwrap();
            println!(
                "{}",
                json!({"pid":std::process::id(),"capture":"actual native eframe WGPU Screenshot event","pixels":image.size,"p6":p,"p7":rgba,"original_color32_rgba_retained":true,"full_session_unchanged":true,"document_status_unchanged":true,"native_bytes_unchanged":true,"os_input":false,"self_close_after_capture":true,"controls_and_effective_clips":controls.iter().map(|(r,c)|[[r.min.x,r.min.y,r.max.x,r.max.y],[c.min.x,c.min.y,c.max.x,c.max.y]]).collect::<Vec<_>>() })
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.passes < 180);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        input.focused = true;
        input.events.retain(|e| !matches!(e, egui::Event::WindowFocused(false)));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 5, "wide|veto|recovery NEW P6 NEW native NEW state");
    let mode = &args[1];
    assert!(["wide", "veto", "recovery"].contains(&mode.as_str()));
    let capture = PathBuf::from(&args[2]);
    let native = PathBuf::from(&args[3]);
    let state = PathBuf::from(&args[4]);
    assert!(!capture.exists() && !native.exists() && !state.exists());
    let e = EngineHandle::new(Project::new("Command hooks native inspection"));
    e.execute("add_via", json!({"x_mm":20,"y_mm":20,"diameter_mm":1,"drill_mm":0.4,"net":"A"}), Origin::User)?;
    e.save_project(viamind_engine::SaveProject { path: Some(native.clone()), overwrite: false })?;
    let bytes = std::fs::read(&native)?;
    let original = e.session_state();
    let original_status = e.document_status();
    let auto = e.auto_drc_status();
    let mut app = ViamindApp::with_design(e.clone(), None);
    app.begin_command_hooks();
    let size = if mode == "wide" { egui::vec2(1600., 1000.) } else { egui::vec2(640., 400.) };
    let mut h = Harness::builder().with_size(size).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(6);
    for label in ["Enable scoped DRC veto", "Apply hook settings"] {
        h.get_by_label(label).scroll_to_me();
        h.run_steps(4);
        h.get_by_label(label).click();
        h.run_steps(5);
    }
    assert!(e.command_hook_status().config.enabled);
    assert_eq!(e.session_state(), original);
    assert_eq!(e.document_status(), original_status);
    let result = e.execute("add_part", json!({"designator":"VETO","value":"legacy candidate","footprint":"F","x_mm":20,"y_mm":20}), Origin::User);
    assert!(result.is_err());
    assert_eq!(e.session_state(), original);
    assert_eq!(e.document_status(), original_status);
    assert_eq!(e.auto_drc_status(), auto);
    h.run_steps(5);
    if mode == "recovery" {
        h.get_by_label("Disable veto").click();
        h.run_steps(5);
        assert!(!e.command_hook_status().config.enabled);
        e.execute("add_via", json!({"x_mm":30,"y_mm":30,"diameter_mm":1,"drill_mm":0.4,"net":"B"}), Origin::User)?;
        assert_eq!(e.event_cursor().sequence, original.cursor.sequence + 1);
        e.undo(Origin::User)?;
        assert_eq!(e.session_state().project, original.project);
        assert!(!e.document_status().dirty);
        h.run_steps(5);
    }
    let before = e.session_state();
    let status = e.document_status();
    assert_eq!(std::fs::read(&native)?, bytes);
    std::fs::write(
        state,
        serde_json::to_vec_pretty(
            &json!({"mode":mode,"original":original,"capture_state":before,"status":status,"hooks":e.command_hook_status(),"native_unchanged":true,"actual_local_input":true,"os_input":false,"veto_rollback":true,"recovery_add_one_event_then_undo":mode=="recovery"}),
        )?,
    )?;
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind command hooks inspection")
            .with_app_id("viamind-command-hooks-inspection")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind command hooks inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspection { app, engine: e, before, status, native, bytes, capture: Some(capture), passes: 0 }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
