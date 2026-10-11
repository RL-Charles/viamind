//! Saved detached settings and bounded coherent manufacturing release exports.
use crate::{ViamindApp, actions::UiAction};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};
use viamind_engine::{EngineError, EngineHandle, EventCursor};
const CHECK_LABELS: [&str; 7] = [
    "Copper clearance baseline (mm)",
    "Edge clearance (mm)",
    "Minimum via drill (mm)",
    "Minimum annular width (mm)",
    "Hole clearance (mm)",
    "Hole to copper clearance (mm)",
    "Maximum DRC violations",
];
use viamind_fab::{OutputJob, OutputJobChecks, OutputJobFileRequest, OutputJobSnapshot, RunOutputJobRequest, SaveOutputJobRequest};
fn invalid(error: impl std::fmt::Display) -> EngineError {
    EngineError::InvalidInput { command: "output_job".into(), message: error.to_string() }
}
pub(crate) fn load(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let _ = engine;
    let input: OutputJobFileRequest = serde_json::from_value(input).map_err(invalid)?;
    Ok(json!({"job":viamind_fab::load_output_job(&input.path).map_err(invalid)?}))
}
pub(crate) fn save(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input: SaveOutputJobRequest = serde_json::from_value(input).map_err(invalid)?;
    let path = viamind_fab::save_output_job(&input.job, &input.path, engine.document_status().path.as_deref()).map_err(invalid)?;
    Ok(json!({"path":path,"job":input.job}))
}
pub(crate) fn run(engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let input: RunOutputJobRequest<EventCursor> = serde_json::from_value(input).map_err(invalid)?;
    let job = viamind_fab::load_output_job(&input.job_path).map_err(invalid)?;
    let (snapshot, cursor, native) = engine.with_engine(|e| {
        let cursor = e.event_cursor();
        if input.expected_cursor.is_some_and(|c| c != cursor) {
            return Err(invalid("document changed; obtain a fresh cursor and retry output job"));
        }
        Ok((OutputJobSnapshot::capture(e.project(), job).map_err(invalid)?, cursor, e.document_status().path))
    })?;
    let (path, manifest) =
        snapshot.write_with_validation(cursor, &input.path, native.as_deref(), viamind_engine::checked_output_job_report).map_err(invalid)?;
    Ok(json!({"path":path,"manifest":manifest,"exported_cursor":cursor,"file_count":manifest.files.len()+1}))
}
#[derive(Default)]
pub struct OutputJobUi {
    open: bool,
    name: String,
    mask: String,
    checked: bool,
    checks: [String; 7],
    job_path: String,
    release_path: String,
    worker: Option<Receiver<Result<Value, String>>>,
    message: Option<String>,
    result: Option<Value>,
    loaded: Option<OutputJob>,
}
impl OutputJobUi {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn config(&self) -> Result<OutputJob, String> {
        let mask = self.mask.parse::<f64>().map_err(|_| "Mask expansion needs a finite value from 0 to 10 mm".to_string())?;
        let mut job = OutputJob::new(self.name.clone(), mask);
        if self.checked {
            let parse =
                |index: usize| self.checks[index].parse::<f64>().map_err(|_| format!("{} needs a finite number in 0..100 mm", CHECK_LABELS[index]));
            job.profile = viamind_fab::CHECKED_PROFILE.into();
            job.checks = Some(OutputJobChecks {
                copper_clearance_mm: parse(0)?,
                edge_clearance_mm: parse(1)?,
                minimum_via_drill_mm: parse(2)?,
                minimum_annular_width_mm: parse(3)?,
                hole_clearance_mm: parse(4)?,
                hole_to_copper_clearance_mm: parse(5)?,
                max_violations: self.checks[6].parse().map_err(|_| "Maximum DRC violations needs an integer in 1..10000".to_string())?,
            });
        }
        job.validate().map_err(|e| e.to_string())?;
        Ok(job)
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else { return };
        match worker.try_recv() {
            Ok(result) => {
                self.worker = None;
                match result {
                    Ok(value) => {
                        if let Some(job) = value.get("job") {
                            match serde_json::from_value::<OutputJob>(job.clone()) {
                                Ok(job) => {
                                    self.name = job.name.clone();
                                    self.mask = job.mask_expansion_mm.to_string();
                                    self.checked = job.checks.is_some();
                                    if let Some(checks) = &job.checks {
                                        self.checks = [
                                            checks.copper_clearance_mm.to_string(),
                                            checks.edge_clearance_mm.to_string(),
                                            checks.minimum_via_drill_mm.to_string(),
                                            checks.minimum_annular_width_mm.to_string(),
                                            checks.hole_clearance_mm.to_string(),
                                            checks.hole_to_copper_clearance_mm.to_string(),
                                            checks.max_violations.to_string(),
                                        ];
                                    }
                                    self.loaded = Some(job);
                                }
                                Err(e) => {
                                    self.message = Some(format!("Invalid worker result: {e}"));
                                    return;
                                }
                            }
                        }
                        self.message = None;
                        self.result = Some(value);
                    }
                    Err(e) => {
                        self.message = Some(format!("Output job failed: {e}"));
                        self.result = None;
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.result = None;
                self.message = Some("Output job worker disconnected. Check the destination before retrying.".into());
            }
        }
    }
    fn start(&mut self, action: UiAction, engine: EngineHandle, ctx: egui::Context) {
        self.start_with(action, engine, ctx, |task| std::thread::Builder::new().name("viamind-output-job".into()).spawn(task).map(|_| ()));
    }
    fn start_with(
        &mut self,
        action: UiAction,
        engine: EngineHandle,
        ctx: egui::Context,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        if self.busy() {
            return;
        }
        let request = (|| {
            if self.job_path.trim().is_empty() {
                return Err("Enter a saved job file path.".to_string());
            }
            match action {
                UiAction::LoadOutputJob => Ok(json!({"path":self.job_path})),
                UiAction::SaveOutputJob => Ok(json!({"path":self.job_path,"job":self.config()?})),
                _ => {
                    if self.release_path.trim().is_empty() {
                        return Err("Enter a NEW release directory path.".to_string());
                    }
                    Ok(json!({"job_path":self.job_path,"path":self.release_path,"expected_cursor":engine.event_cursor()}))
                }
            }
        })();
        let input = match request {
            Ok(input) => input,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        match spawn(Box::new(move || {
            let result = action.run(&engine, input).map_err(|e| e.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        })) {
            Ok(()) => {
                self.worker = Some(receiver);
                self.message = None;
                self.result = None;
            }
            Err(e) => self.message = Some(format!("Could not start output job worker: {e}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_output_jobs(&mut self) {
        if self.design.is_some() {
            self.output_jobs.open = true;
            if self.output_jobs.name.is_empty() {
                self.output_jobs.name = "Manufacturing release".into();
                self.output_jobs.mask = "0".into();
                self.output_jobs.checks = ["0.2", "0.25", "0.2", "0.1", "0.25", "0.2", "1000"].map(str::to_owned);
            }
        }
    }
    pub(crate) fn output_jobs_window(&mut self, ui: &mut egui::Ui) {
        self.output_jobs.poll(ui.ctx());
        if !self.output_jobs.open {
            return;
        }
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let cursor = engine.event_cursor();
        let busy = self.output_jobs.busy();
        let mut action = None;
        let mut open = self.output_jobs.open;
        egui::Window::new("Saved output jobs").open(&mut open).default_width(580.).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("One board snapshot: generic BOM/placement, outer Gerber X2, mask, profile, through-via XNC and manifests.");
            ui.label("Settings are detached JSON. Run reads the saved file, including changes made outside this window.");
            ui.add_enabled_ui(!busy, |ui| {
                let label = ui.label("Job name");
                ui.add(egui::TextEdit::singleline(&mut self.output_jobs.name).char_limit(256).desired_width(500.)).labelled_by(label.id);
                let label = ui.label("Mask expansion (mm)");
                ui.add(egui::TextEdit::singleline(&mut self.output_jobs.mask).char_limit(32)).labelled_by(label.id);
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut self.output_jobs.checked, false, "Generic outer 2-layer");
                    ui.radio_value(&mut self.output_jobs.checked, true, "Checked outer 2-layer");
                });
                if self.output_jobs.checked {
                    ui.label("Refuse release on scoped DRC violations, incomplete coverage or excluded geometry. Uses the explicit saved check baselines.");
                    for (index, label) in CHECK_LABELS.iter().enumerate() {
                        let label = ui.label(*label);
                        ui.add(egui::TextEdit::singleline(&mut self.output_jobs.checks[index]).char_limit(32)).labelled_by(label.id);
                    }
                }
                let label = ui.label("Saved job file");
                ui.add(egui::TextEdit::singleline(&mut self.output_jobs.job_path).char_limit(4096).desired_width(500.)).labelled_by(label.id);
                ui.horizontal(|ui| {
                    if ui.button("Load job").clicked() {
                        action = Some(UiAction::LoadOutputJob);
                    }
                    if ui.button("Save NEW job file").clicked() {
                        action = Some(UiAction::SaveOutputJob);
                    }
                });
                if self.output_jobs.loaded.as_ref() != self.output_jobs.config().ok().as_ref() {
                    ui.label("Form settings differ from the last loaded/saved job. Save to a NEW file to use them.");
                }
                let label = ui.label("NEW release directory");
                ui.add(egui::TextEdit::singleline(&mut self.output_jobs.release_path).char_limit(4096).desired_width(500.)).labelled_by(label.id);
                ui.label("Existing files and directories are preserved. At most 100000 components, 200000 board primitives and 192 MiB release.");
                ui.label("Requires explicit pads and outline. Scoped checks exclude connectivity, mask, silk and complete DFM. No inner copper, pours, sourcing metadata or fabrication certification.");
                if ui.button("Run saved job to NEW directory").clicked() {
                    action = Some(UiAction::RunOutputJob);
                }
            });
            if busy {
                ui.spinner();
                ui.label("Working on output job…");
            }
            if let Some(message) = &self.output_jobs.message {
                ui.label(message);
            }
            if let Some(result) = &self.output_jobs.result {
                if let Some(path) = result["path"].as_str() {
                    ui.label(format!("Wrote {path}"));
                }
                if result.get("job").is_some() {
                    ui.label("Job settings loaded/saved. Run reads the saved job file.");
                }
                if result.get("exported_cursor").is_some() {
                    ui.label(format!(
                        "{} files · snapshot {} / {}",
                        result["file_count"],
                        result["exported_cursor"]["stream_id"].as_str().unwrap_or(""),
                        result["exported_cursor"]["sequence"]
                    ));
                    if result["exported_cursor"] != json!(cursor) {
                        ui.label("The design has changed since this exported snapshot.");
                    }
                }
            }
        });
        self.output_jobs.open = open;
        if let Some(action) = action {
            self.output_jobs.start(action, engine, ui.ctx().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_hidden_worker_duplicate_disconnect_and_fallible_spawn() {
        let (tx, rx) = mpsc::channel();
        let ctx = egui::Context::default();
        let engine = EngineHandle::new(viamind_doc::Project::new("unchanged"));
        let before = engine.session_state();
        let mut state = OutputJobUi { worker: Some(rx), ..Default::default() };
        state.start_with(UiAction::LoadOutputJob, engine.clone(), ctx.clone(), |_| panic!("duplicate spawn"));
        state.poll(&ctx);
        assert!(state.busy());
        assert!(!state.open);
        tx.send(Ok(json!({"path":"complete"}))).unwrap();
        state.poll(&ctx);
        assert!(!state.busy());
        assert_eq!(state.result.as_ref().unwrap()["path"], "complete");
        let (tx, rx) = mpsc::channel();
        state.worker = Some(rx);
        drop(tx);
        state.poll(&ctx);
        assert!(!state.busy());
        assert!(state.message.as_ref().unwrap().contains("disconnected"));
        state.job_path = "new-job.json".into();
        state.name = "Job".into();
        state.mask = "0.1".into();
        state.start_with(UiAction::SaveOutputJob, engine.clone(), ctx, |_| Err(std::io::Error::other("controlled spawn failure")));
        assert!(!state.busy());
        assert!(state.message.as_ref().unwrap().contains("controlled spawn failure"));
        assert_eq!(state.job_path, "new-job.json");
        assert_eq!(state.name, "Job");
        assert_eq!(state.mask, "0.1");
        assert_eq!(engine.session_state(), before);
    }
    #[test]
    fn pending_fields_and_all_buttons_are_disabled_then_enabled() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let (tx, rx) = mpsc::channel();
        let engine = EngineHandle::new(viamind_doc::Project::new("pending"));
        let mut app = ViamindApp::with_design(engine, None);
        app.begin_output_jobs();
        app.output_jobs.worker = Some(rx);
        app.output_jobs.checked = true;
        let mut h = Harness::builder().build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
        h.run_steps(3);
        for label in [
            "Job name",
            "Mask expansion (mm)",
            "Saved job file",
            "NEW release directory",
            "Load job",
            "Save NEW job file",
            "Run saved job to NEW directory",
        ] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        }
        for label in CHECK_LABELS.iter().copied().chain(["Generic outer 2-layer", "Checked outer 2-layer"]) {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        }
        tx.send(Err("controlled failure".into())).unwrap();
        h.run_steps(3);
        for label in ["Job name", "Load job", "Run saved job to NEW directory"] {
            assert!(!h.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        }
    }
}
