//! Authored isolated branch, actual local UI review, native WGPU readback and self-close.
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use viamind_doc::Project;
use viamind_engine::{DocumentStatus, EngineHandle, Origin, SessionSnapshot, pcb_branches::*};
use viamind_ui_egui::ViamindApp;

struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    passes: usize,
    merged: bool,
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
        self.passes += 1;
        let content = ui.ctx().content_rect();
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(self.passes >= 8 && image.pixels.len() <= 4_194_304);
            if !self.merged {
                let controls = self.app.pcb_branches.control_rects();
                assert_eq!(controls.len(), 3);
                assert!(
                    controls.iter().all(|(r, clip)| content.contains_rect(*r) && clip.contains_rect(*r)),
                    "actual native current-pass review controls must fit effective clips: {controls:?}"
                );
                assert!(self.app.pcb_branches.visible());
                assert!(self.app.pcb_branches.ghost_items() > 10);
            } else {
                assert!(!self.app.pcb_branches.visible());
                assert!(self.app.pcb_branches.message().unwrap().contains("one undoable"));
            }
            let path = self.capture.take().unwrap();
            let rgba_path = path.with_extension("p7");
            let mut rgba = std::fs::OpenOptions::new().write(true).create_new(true).open(&rgba_path).unwrap();
            write!(rgba, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1]).unwrap();
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b(), p.a()]).collect();
            rgba.write_all(&bytes).unwrap();
            rgba.sync_all().unwrap();
            let mut rgb = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            write!(rgb, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            rgb.write_all(&bytes).unwrap();
            rgb.sync_all().unwrap();
            println!(
                "{}",
                json!({"capture":"actual eframe native WGPU Screenshot event","pid":std::process::id(),"path":path,"rgba_path":rgba_path,"pixels":image.size,"ui_passes":self.passes,"full_session_unchanged":true,"document_status_unchanged":true,"merged":self.merged,"self_close_after_capture":true,"original_color32_rgba_retained":true,"alpha_min":image.pixels.iter().map(egui::Color32::a).min(),"alpha_max":image.pixels.iter().map(egui::Color32::a).max(),"nonopaque_alpha_pixels":image.pixels.iter().filter(|p|p.a()!=255).count(),"rgb_review":"original Color32 RGB samples; alpha omitted for review","native_requested_transparent":false,"controls_and_effective_clips":self.app.pcb_branches.control_rects().iter().map(|(r,c)|[[r.min.x,r.min.y,r.max.x,r.max.y],[c.min.x,c.min.y,c.max.x,c.max.y]]).collect::<Vec<_>>(),"content_rect":[content.min.x,content.min.y,content.max.x,content.max.y],"os_input":false})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.passes < 180, "native screenshot timeout");
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        // Keep only this self-closing inspection's logical local input focus.
        input.focused = true;
        input.events.retain(|e| !matches!(e, egui::Event::WindowFocused(false)));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 5, "mode ghost|compact|merged, NEW P6 path, NEW native path, NEW state report");
    let mode = &args[1];
    assert!(matches!(mode.as_str(), "ghost" | "compact" | "merged"));
    let capture = PathBuf::from(&args[2]);
    let native = PathBuf::from(&args[3]);
    let report = PathBuf::from(&args[4]);
    assert!(!capture.exists() && !native.exists() && !report.exists());
    let engine = EngineHandle::new(Project::new("Authored PCB branch visual proof"));
    let part = engine
        .execute(
            "add_part",
            json!({"designator":"U1","value":"live geometry","footprint":"authored","x_mm":40,"y_mm":30,"rotation_deg":90,"side":"bottom"}),
            Origin::User,
        )?
        .created[0];
    engine.execute("set_part_pads",json!({"id":part,"expected":engine.read(|p|p.board.parts[&part].clone()),"pads":[{"number":"1","x_mm":-1.000003,"y_mm":0.000007,"width_mm":0.8,"height_mm":0.6,"net":"N"},{"number":"2","x_mm":1.000003,"y_mm":0.000007,"width_mm":0.8,"height_mm":0.6,"net":"N"}]}),Origin::User)?;
    let track = engine
        .execute(
            "add_track",
            json!({"start_x_mm":30,"start_y_mm":30,"end_x_mm":40,"end_y_mm":30,"width_mm":0.4,"layer":"top","net":"N"}),
            Origin::User,
        )?
        .created[0];
    engine.save_project(viamind_engine::SaveProject { path: Some(native.clone()), overwrite: false })?;
    let native_bytes = std::fs::read(&native)?;
    let original = engine.session_state();
    let original_status = engine.document_status();
    let actor = BranchActor::agent(1, "native authored branch");
    let b = engine.create_pcb_branch(
        CreatePcbBranch {
            expected_project: original.project.id,
            expected_cursor: original.cursor,
            label: "Move U1, replace copper, add U2 and via".into(),
        },
        actor.clone(),
        reserve_pcb_branch_work()?,
    )?;
    let calls = vec![
        ("move_part", json!({"id":part,"x_mm":48,"y_mm":38})),
        ("delete_track", json!({"id":track})),
        ("add_track", json!({"start_x_mm":43,"start_y_mm":38,"end_x_mm":55,"end_y_mm":38,"width_mm":0.4,"layer":"bottom","net":"N"})),
        ("add_via", json!({"x_mm":55,"y_mm":38,"diameter_mm":1,"drill_mm":0.4,"net":"N"})),
        ("add_part", json!({"designator":"U2","value":"private component","footprint":"authored","x_mm":57,"y_mm":42})),
    ]
    .into_iter()
    .map(|(name, input)| PcbBranchCall { name: name.into(), input })
    .collect();
    engine.apply_pcb_branch(ApplyPcbBranch { branch_id: b.branch_id, expected_revision: 0, commands: calls }, &actor, reserve_pcb_branch_work()?)?;
    assert_eq!(engine.session_state(), original);
    assert_eq!(engine.document_status(), original_status);
    let size = if mode == "compact" { egui::vec2(640., 400.) } else { egui::vec2(1600., 1000.) };
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_pcb_branches();
    let mut h = Harness::builder().with_size(size).build_ui_state(|ui, app: &mut ViamindApp| app.show(ui, None), app);
    h.run_steps(6);
    h.get_by_label("Inspect diff").scroll_to_me();
    h.run_steps(4);
    h.get_by_label("Inspect diff").click();
    h.run_steps(5);
    assert!(h.state().pcb_branches.visible());
    h.get_by_label("Merge branch").scroll_to_me();
    h.run_steps(4);
    if mode == "merged" {
        h.get_by_label("Merge branch").click();
        h.run_steps(5);
        h.get_by_label_contains("Branch merged as one undoable").scroll_to_me();
        h.run_steps(4);
    }
    let before = engine.session_state();
    let status = engine.document_status();
    assert_eq!(std::fs::read(&native)?, native_bytes);
    std::fs::write(
        report,
        serde_json::to_vec_pretty(
            &json!({"original":original,"after_preparation_or_merge":before,"status":status,"native_unchanged":true,"mode":mode,"actual_local_inspect_and_merge_inputs":true,"os_input":false}),
        )?,
    )?;
    let merged = mode == "merged";
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind PCB branch inspection")
            .with_app_id("viamind-pcb-branches-inspection")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false)
            .with_transparent(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind PCB branch inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture: Some(capture), passes: 0, merged }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
