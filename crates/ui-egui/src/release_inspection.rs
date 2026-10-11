//! A detached historical release reader; no design capture or mutation.
use crate::{ViamindApp, actions::UiAction};
use serde_json::Value;
use std::sync::mpsc::{self, Receiver};
use viamind_engine::{EngineError, EngineHandle};
pub(crate) fn inspect(_engine: &EngineHandle, input: Value) -> Result<Value, EngineError> {
    let invalid = |error: String| EngineError::InvalidInput { command: "inspect_output_release".into(), message: error };
    if input.get("path").and_then(Value::as_str).is_some_and(|path| path.len() > 4096) {
        return Err(invalid("release path exceeds4096 UTF-8 bytes before decoding".into()));
    }
    let input: viamind_fab::InspectOutputRelease = serde_json::from_value(input).map_err(|e| invalid(e.to_string()))?;
    serde_json::to_value(viamind_fab::inspect_output_release(&input).map_err(|e| invalid(e.to_string()))?).map_err(|e| invalid(e.to_string()))
}
#[derive(Default)]
pub struct ReleaseInspectionUi {
    pub open: bool,
    pub path: String,
    pub result: Option<Value>,
    pub message: Option<String>,
    worker: Option<Receiver<Result<Value, String>>>,
}
impl ReleaseInspectionUi {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else { return };
        match worker.try_recv() {
            Ok(result) => {
                self.worker = None;
                match result {
                    Ok(result) => {
                        self.result = Some(result);
                        self.message = None;
                    }
                    Err(error) => {
                        self.result = None;
                        self.message = Some(error);
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.result = None;
                self.message = Some("Release inspection worker disconnected; retry.".into());
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(30)),
        }
    }
    fn start(&mut self, engine: EngineHandle, ctx: egui::Context) {
        self.start_with(engine, ctx, |work| std::thread::Builder::new().name("viamind-release-inspection".into()).spawn(work).map(|_| ()));
    }
    fn start_with(&mut self, engine: EngineHandle, ctx: egui::Context, spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>) {
        if self.busy() {
            return;
        }
        if self.path.is_empty() || self.path.len() > 4096 {
            self.message = Some("Enter an explicit release directory path (1–4096 UTF-8 bytes).".into());
            return;
        }
        let input = serde_json::json!({"path":self.path});
        let (sender, receiver) = mpsc::channel();
        self.result = None;
        match spawn(Box::new(move || {
            let result = UiAction::InspectOutputRelease.run(&engine, input).map_err(|e| e.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        })) {
            Ok(()) => {
                self.worker = Some(receiver);
                self.message = None;
            }
            Err(error) => self.message = Some(format!("Could not start release inspection worker: {error}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_release_inspection(&mut self) {
        if self.design.is_some() {
            self.release_inspection.open = true;
        }
    }
    pub(crate) fn release_inspection_window(&mut self, ui: &mut egui::Ui) {
        self.release_inspection.poll(ui.ctx());
        if !self.release_inspection.open {
            return;
        }
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let busy = self.release_inspection.busy();
        let mut open = self.release_inspection.open;
        let mut start = false;
        egui::Window::new("Inspect saved release").open(&mut open).default_width(560.).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Read-only historical release consistency. Existing files and the current document stay unchanged.");
            ui.label("Fixed versions 1–3/layouts only. Per-file inspector limits can refuse otherwise valid larger exports.");
            ui.add_enabled_ui(!busy, |ui| {
                let label = ui.label("Saved release directory");
                let changed = ui
                    .add(egui::TextEdit::singleline(&mut self.release_inspection.path).char_limit(4096).desired_width(520.))
                    .labelled_by(label.id)
                    .changed();
                if changed {
                    self.release_inspection.result = None;
                    self.release_inspection.message = None;
                }
                if ui.button("Inspect release").clicked() {
                    start = true;
                }
            });
            if busy {
                ui.spinner();
                ui.label("Reading saved release…");
            }
            if let Some(message) = &self.release_inspection.message {
                ui.label(message);
            }
            if let Some(result) = &self.release_inspection.result {
                ui.separator();
                ui.label(format!(
                    "Version {} / {} · {} files · {} observed bytes",
                    result["version"],
                    result["profile"].as_str().unwrap_or(""),
                    result["file_count"],
                    result["observed_bytes"]
                ));
                ui.label(format!("Historical project {}", result["project_id"].as_str().unwrap_or("")));
                ui.label(format!(
                    "Exported cursor {} / {}",
                    result["exported_cursor"]["stream_id"].as_str().unwrap_or(""),
                    result["exported_cursor"]["sequence"]
                ));
                ui.label("Historical provenance is independent of the current board; no equality claim.");
                if let Some(count) = result["calibrated_component_count"].as_u64() {
                    ui.label(format!("Calibrated rows: {count}"));
                }
                if let Some(count) = result["matched_assembly"]["component_count"].as_u64() {
                    ui.label(format!("Matched populated component IDs: {count}"));
                    ui.label(format!("Component identity SHA256: {}", result["matched_assembly"]["component_ids_sha256"].as_str().unwrap_or("")));
                }
                for (key, title) in [
                    ("checks_performed", "Checks performed"),
                    ("checks_not_performed", "Checks not performed"),
                    ("release_exclusions", "Release-declared exclusions"),
                ] {
                    ui.strong(title);
                    if let Some(rows) = result[key].as_array() {
                        for row in rows {
                            ui.label(row.as_str().unwrap_or(""));
                        }
                    }
                }
            }
        });
        self.release_inspection.open = open;
        if start {
            self.release_inspection.start(engine, ui.ctx().clone());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spawn_failure_duplicate_hidden_and_disconnect_are_recoverable() {
        let engine = EngineHandle::new(viamind_doc::Project::new("reader controls"));
        let before = engine.session_state();
        let ctx = egui::Context::default();
        let mut state = ReleaseInspectionUi { path: "explicit/path".into(), ..Default::default() };
        state.start_with(engine.clone(), ctx.clone(), |_| Err(std::io::Error::other("injected")));
        assert!(!state.busy());
        assert!(state.message.as_deref().unwrap().contains("injected"));
        let mut work = None;
        state.start_with(engine.clone(), ctx.clone(), |w| {
            work = Some(w);
            Ok(())
        });
        assert!(state.busy());
        state.start_with(engine.clone(), ctx.clone(), |_| panic!("duplicate"));
        state.open = false;
        work.take().unwrap()();
        state.poll(&ctx);
        assert!(!state.busy());
        assert!(state.message.is_some());
        state.start_with(engine.clone(), ctx.clone(), |_| Ok(()));
        state.poll(&ctx);
        assert!(!state.busy());
        assert!(state.message.as_deref().unwrap().contains("disconnected"));
        assert_eq!(engine.session_state(), before);
    }
}
